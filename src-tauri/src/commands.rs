//! User-defined ffmpeg commands: data model, persistence, argv resolution and
//! Markdown export/import. Rust owns this list; the frontend only mirrors it via
//! `get_commands` + the `commands-updated` event.

use crate::actions::next_available_path;
use crate::context_menu::sync_context_menu;
use crate::settings::STORE_FILENAME;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

const COMMANDS_KEY: &str = "commands";

/// Serializes read-modify-write cycles on the command list. Tauri may run
/// command handlers concurrently, and the store has no compare-and-swap.
static COMMANDS_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Video,
    Audio,
    Image,
    Subtitle,
}

impl FileCategory {
    pub const ALL: [FileCategory; 4] = [
        FileCategory::Video,
        FileCategory::Audio,
        FileCategory::Image,
        FileCategory::Subtitle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FileCategory::Video => "Video",
            FileCategory::Audio => "Audio",
            FileCategory::Image => "Image",
            FileCategory::Subtitle => "Subtitle",
        }
    }

    /// Registry targets under `HKCU\Software\Classes\SystemFileAssociations`.
    /// The perceived type (`video`/`audio`/`image`) catches anything Windows
    /// classifies that way; explicit extensions keep working when a third-party
    /// player hijacks the association. Lists are disjoint so a file never gets
    /// two FFMPEG-Toolkit menus.
    /// `.ts` is deliberately absent: on a dev box it is TypeScript far more often than MPEG-TS.
    pub fn registry_targets(self) -> &'static [&'static str] {
        match self {
            FileCategory::Video => &[
                "video", ".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm", ".m4v", ".mpg",
                ".mpeg", ".3gp", ".mts", ".m2ts", ".ogv",
            ],
            FileCategory::Audio => &[
                "audio", ".mp3", ".wav", ".flac", ".aac", ".m4a", ".ogg", ".opus", ".wma", ".aiff",
                ".ac3",
            ],
            FileCategory::Image => &[
                "image", ".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif", ".tif", ".tiff", ".avif",
            ],
            FileCategory::Subtitle => &[".srt", ".ass", ".ssa", ".vtt", ".sub"],
        }
    }

    /// Extensions without the dot, used for the "Run on files…" picker filter.
    pub fn extensions(self) -> Vec<String> {
        self.registry_targets()
            .iter()
            .filter_map(|t| t.strip_prefix('.'))
            .map(str::to_string)
            .collect()
    }

    fn from_heading(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().trim_end_matches('s') {
            "video" => Some(FileCategory::Video),
            "audio" => Some(FileCategory::Audio),
            "image" => Some(FileCategory::Image),
            "subtitle" => Some(FileCategory::Subtitle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomCommand {
    pub id: String,
    pub category: FileCategory,
    pub label: String,
    pub args: String,
    pub output_name: String,
    pub output_ext: String,
    pub enabled: bool,
    /// Optional Explorer submenu name ("" = directly under FFMPEG-Toolkit ▶).
    /// `serde(default)` so stores written before this field existed still load.
    #[serde(default)]
    pub group: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryInfo {
    pub id: FileCategory,
    pub label: &'static str,
    pub extensions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tokenizer + placeholder resolution
// ---------------------------------------------------------------------------

/// Quote-aware argv splitter. Whitespace (incl. newlines) separates tokens,
/// `"..."` and `'...'` group, and backslashes are LITERAL — these are Windows
/// paths, so POSIX shlex semantics (`\` escapes) would mangle `C:\Videos`.
pub fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_token = false; // distinguishes `""` (empty arg) from "no token"
    let mut quote: Option<char> = None;

    for ch in input.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                in_token = true;
            }
            None if ch.is_whitespace() => {
                if in_token {
                    tokens.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            None => {
                cur.push(ch);
                in_token = true;
            }
        }
    }
    if let Some(q) = quote {
        return Err(format!("Unclosed {} quote in arguments", q));
    }
    if in_token {
        tokens.push(cur);
    }
    Ok(tokens)
}

const ILLEGAL_FILENAME_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

pub fn validate(cmd: &CustomCommand) -> Result<(), String> {
    if cmd.label.trim().is_empty() {
        return Err("Menu text cannot be empty.".into());
    }
    if cmd.label.contains('\n') {
        return Err("Menu text must be a single line.".into());
    }
    if cmd.group.contains('\n') {
        return Err("Submenu name must be a single line.".into());
    }
    let tokens = tokenize(&cmd.args)?;
    if !tokens.iter().any(|t| t.contains("{input}")) {
        return Err("Arguments must contain {input}.".into());
    }
    if cmd.output_ext.is_empty() || !cmd.output_ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Output extension must be letters/digits only (e.g. mp4).".into());
    }
    if cmd.output_name.trim().is_empty() {
        return Err("Output name cannot be empty (use {name} for the source name).".into());
    }
    if cmd.output_name.contains(ILLEGAL_FILENAME_CHARS) {
        return Err("Output name contains characters not allowed in file names.".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedInvocation {
    /// User argv after placeholder substitution (no ffmpeg binary, no progress flags).
    pub argv: Vec<String>,
    /// `Some` only when the args reference `{output}`.
    pub output_path: Option<String>,
    pub warnings: Vec<String>,
}

/// Resolves a command against a concrete input file. Tokenize FIRST, substitute
/// SECOND: a path with spaces then stays a single argv entry with zero quoting,
/// because we spawn ffmpeg directly (no shell re-splits it).
pub fn resolve(cmd: &CustomCommand, input_path: &str) -> Result<ResolvedInvocation, String> {
    validate(cmd)?;
    let input = Path::new(input_path);
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    let in_ext = input
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut tokens = tokenize(&cmd.args)?;
    // Forgive users who paste the whole line including the binary name.
    if tokens
        .first()
        .map(|t| t.eq_ignore_ascii_case("ffmpeg") || t.eq_ignore_ascii_case("ffmpeg.exe"))
        .unwrap_or(false)
    {
        tokens.remove(0);
    }

    let uses_output = tokens.iter().any(|t| t.contains("{output}"));
    let mut warnings = Vec::new();
    let output_path = if uses_output {
        let out_stem = cmd.output_name.replace("{name}", &stem);
        Some(
            next_available_path(dir, &out_stem, &cmd.output_ext)
                .to_string_lossy()
                .to_string(),
        )
    } else {
        warnings.push(
            "Arguments don't contain {output}; the output file won't be tracked or protected from overwrite."
                .into(),
        );
        None
    };

    let dir_s = dir.to_string_lossy();
    let argv = tokens
        .into_iter()
        .map(|t| {
            let t = t
                .replace("{input}", input_path)
                .replace("{dir}", &dir_s)
                .replace("{name}", &stem)
                .replace("{ext}", &in_ext);
            match &output_path {
                Some(o) => t.replace("{output}", o),
                None => t,
            }
        })
        .collect();

    Ok(ResolvedInvocation {
        argv,
        output_path,
        warnings,
    })
}

/// Human-readable command line for previews / error reports.
pub fn display_argv(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.is_empty() || a.contains(char::is_whitespace) {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Markdown export / import
// ---------------------------------------------------------------------------

pub fn render_markdown(cmds: &[CustomCommand]) -> String {
    let mut out = String::from("# FFMPEG-Toolkit Commands\n");
    for cat in FileCategory::ALL {
        let in_cat: Vec<_> = cmds.iter().filter(|c| c.category == cat).collect();
        if in_cat.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {}\n", cat.label()));
        for c in in_cat {
            // Only written when set, so exports of ungrouped commands look as before.
            let group = if c.group.is_empty() { String::new() } else { format!("- Submenu: {}\n", c.group) };
            out.push_str(&format!(
                "\n### {}\n- Output: `{}.{}`\n{}- Enabled: {}\n\n```ffmpeg\n{}\n```\n",
                c.label.trim(),
                c.output_name,
                c.output_ext,
                group,
                if c.enabled { "yes" } else { "no" },
                c.args.trim_end()
            ));
        }
    }
    out
}

/// Line-based state machine. Unknown lines are ignored so hand-written notes in
/// the .md survive an import. Every parsed command gets a fresh id.
pub fn parse_markdown(md: &str) -> Vec<CustomCommand> {
    let mut result = Vec::new();
    let mut category: Option<FileCategory> = None;
    let mut current: Option<CustomCommand> = None;
    let mut in_fence = false;
    let mut fence_buf: Vec<&str> = Vec::new();

    let flush = |current: &mut Option<CustomCommand>, result: &mut Vec<CustomCommand>| {
        if let Some(c) = current.take() {
            if !c.args.trim().is_empty() {
                result.push(c);
            }
        }
    };

    for line in md.lines() {
        if in_fence {
            if line.trim_start().starts_with("```") {
                in_fence = false;
                if let Some(c) = current.as_mut() {
                    c.args = fence_buf.join("\n").trim().to_string();
                }
                fence_buf.clear();
            } else {
                fence_buf.push(line);
            }
            continue;
        }

        let t = line.trim();
        if let Some(h) = t.strip_prefix("### ") {
            flush(&mut current, &mut result);
            if let Some(cat) = category {
                current = Some(CustomCommand {
                    id: Uuid::new_v4().to_string(),
                    category: cat,
                    label: h.trim().to_string(),
                    args: String::new(),
                    output_name: "{name}".into(),
                    output_ext: "mp4".into(),
                    enabled: true,
                    group: String::new(),
                });
            }
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut current, &mut result);
            category = FileCategory::from_heading(h);
        } else if t.starts_with("```") {
            in_fence = true;
        } else if let Some(c) = current.as_mut() {
            if let Some(v) = t.strip_prefix("- Output:") {
                let v = v.trim().trim_matches('`');
                if let Some((name, ext)) = v.rsplit_once('.') {
                    c.output_name = name.to_string();
                    c.output_ext = ext.to_string();
                }
            } else if let Some(v) = t.strip_prefix("- Submenu:") {
                c.group = v.trim().to_string();
            } else if let Some(v) = t.strip_prefix("- Enabled:") {
                c.enabled = !matches!(v.trim().to_ascii_lowercase().as_str(), "no" | "false");
            }
        }
    }
    flush(&mut current, &mut result);
    result
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn load_commands(app: &AppHandle) -> Vec<CustomCommand> {
    app.store(STORE_FILENAME)
        .ok()
        .and_then(|s| s.get(COMMANDS_KEY))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Persist, broadcast, and re-sync the registry. Caller must hold `COMMANDS_LOCK`.
fn commit(app: &AppHandle, cmds: Vec<CustomCommand>) -> Result<Vec<CustomCommand>, String> {
    let store = app
        .store(STORE_FILENAME)
        .map_err(|e| format!("Failed to open store: {}", e))?;
    store.set(
        COMMANDS_KEY,
        serde_json::to_value(&cmds).map_err(|e| e.to_string())?,
    );
    store
        .save()
        .map_err(|e| format!("Failed to save commands: {}", e))?;
    let _ = app.emit("commands-updated", &cmds);
    // Registry failure shouldn't roll back the saved command; surface it instead.
    sync_context_menu(app, &cmds)?;
    Ok(cmds)
}

fn with_commands<F>(app: &AppHandle, f: F) -> Result<Vec<CustomCommand>, String>
where
    F: FnOnce(&mut Vec<CustomCommand>) -> Result<(), String>,
{
    let _guard = COMMANDS_LOCK.lock().map_err(|_| "Command store lock poisoned")?;
    let mut cmds = load_commands(app);
    f(&mut cmds)?;
    commit(app, cmds)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_categories() -> Vec<CategoryInfo> {
    FileCategory::ALL
        .iter()
        .map(|&c| CategoryInfo {
            id: c,
            label: c.label(),
            extensions: c.extensions(),
        })
        .collect()
}

#[tauri::command]
pub fn get_commands(app: AppHandle) -> Vec<CustomCommand> {
    load_commands(&app)
}

#[tauri::command]
pub fn save_command(app: AppHandle, command: CustomCommand) -> Result<Vec<CustomCommand>, String> {
    let mut command = command;
    command.label = command.label.trim().to_string();
    command.group = command.group.trim().to_string();
    command.output_ext = command.output_ext.trim().trim_start_matches('.').to_lowercase();
    validate(&command)?;
    with_commands(&app, move |cmds| {
        if command.id.is_empty() {
            command.id = Uuid::new_v4().to_string();
        }
        match cmds.iter_mut().find(|c| c.id == command.id) {
            Some(existing) => *existing = command,
            None => cmds.push(command),
        }
        Ok(())
    })
}

#[tauri::command]
pub fn delete_command(app: AppHandle, id: String) -> Result<Vec<CustomCommand>, String> {
    with_commands(&app, |cmds| {
        cmds.retain(|c| c.id != id);
        Ok(())
    })
}

/// Moves a command up/down among commands of the SAME category (the Vec is
/// global, so we swap with the nearest same-category neighbour).
#[tauri::command]
pub fn move_command(app: AppHandle, id: String, up: bool) -> Result<Vec<CustomCommand>, String> {
    with_commands(&app, |cmds| {
        let Some(i) = cmds.iter().position(|c| c.id == id) else {
            return Ok(());
        };
        let cat = cmds[i].category;
        let neighbour = if up {
            (0..i).rev().find(|&j| cmds[j].category == cat)
        } else {
            (i + 1..cmds.len()).find(|&j| cmds[j].category == cat)
        };
        if let Some(j) = neighbour {
            cmds.swap(i, j);
        }
        Ok(())
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandPreview {
    pub command_line: Option<String>,
    pub output_path: Option<String>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn preview_command(command: CustomCommand, sample_path: String) -> CommandPreview {
    match resolve(&command, &sample_path) {
        Ok(r) => CommandPreview {
            command_line: Some(format!("ffmpeg {}", display_argv(&r.argv))),
            output_path: r.output_path,
            warnings: r.warnings,
            error: None,
        },
        Err(e) => CommandPreview {
            command_line: None,
            output_path: None,
            warnings: Vec::new(),
            error: Some(e),
        },
    }
}

#[tauri::command]
pub fn export_commands_md(app: AppHandle, path: String) -> Result<usize, String> {
    let cmds = load_commands(&app);
    std::fs::write(&path, render_markdown(&cmds))
        .map_err(|e| format!("Failed to write {}: {}", path, e))?;
    Ok(cmds.len())
}

/// First run only: if the store has never held a `commands` key, install every
/// preset. Checking key *presence* (not emptiness) matters: a user who deleted
/// all commands has an empty list and must not get the presets back.
pub fn seed_presets_on_first_run(app: &AppHandle) {
    let Ok(_guard) = COMMANDS_LOCK.lock() else { return };
    let Ok(store) = app.store(STORE_FILENAME) else { return };
    if store.has(COMMANDS_KEY) {
        return;
    }
    let _ = commit(app, crate::presets::all_presets());
}

/// Adds the category's presets that aren't already present (matched by label,
/// so a preset the user tweaked isn't re-added as a twin). Returns how many were added.
#[tauri::command]
pub fn add_presets(app: AppHandle, category: FileCategory) -> Result<usize, String> {
    let mut added = 0;
    with_commands(&app, |cmds| {
        for p in crate::presets::presets_for(category) {
            if !cmds.iter().any(|c| c.category == category && c.label == p.label) {
                cmds.push(p);
                added += 1;
            }
        }
        Ok(())
    })?;
    Ok(added)
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub invalid: Vec<String>,
}

#[tauri::command]
pub fn import_commands_md(app: AppHandle, path: String, replace: bool) -> Result<ImportResult, String> {
    let md = std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    let parsed = parse_markdown(&md);
    let mut res = ImportResult {
        imported: 0,
        skipped: 0,
        invalid: Vec::new(),
    };
    with_commands(&app, |cmds| {
        if replace {
            cmds.clear();
        }
        for c in parsed {
            if let Err(e) = validate(&c) {
                res.invalid.push(format!("{}: {}", c.label, e));
                continue;
            }
            let dup = cmds
                .iter()
                .any(|e| e.category == c.category && e.label == c.label && e.args == c.args);
            if dup {
                res.skipped += 1;
            } else {
                cmds.push(c);
                res.imported += 1;
            }
        }
        Ok(())
    })?;
    Ok(res)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(args: &str) -> CustomCommand {
        CustomCommand {
            id: "x".into(),
            category: FileCategory::Video,
            label: "WhatsApp".into(),
            args: args.into(),
            output_name: "{name}_wa".into(),
            output_ext: "mp4".into(),
            enabled: true,
            group: String::new(),
        }
    }

    #[test]
    fn tokenize_quotes_and_backslashes() {
        let t = tokenize(r#"-i {input} -vf "scale=-2:720, fps=30" -metadata title='a b' C:\out\x.mp4"#)
            .unwrap();
        assert_eq!(
            t,
            vec!["-i", "{input}", "-vf", "scale=-2:720, fps=30", "-metadata", "title=a b", r"C:\out\x.mp4"]
        );
    }

    #[test]
    fn tokenize_empty_quoted_arg_and_newlines() {
        assert_eq!(tokenize("-a \"\"\n  -b").unwrap(), vec!["-a", "", "-b"]);
        assert!(tokenize("-vf \"oops").is_err());
    }

    #[test]
    fn resolve_keeps_spaced_paths_as_one_token() {
        let r = resolve(&cmd("ffmpeg -i {input} -c:v libx264 {output}"), r"C:\My Videos\clip one.mkv").unwrap();
        assert_eq!(r.argv[0], "-i"); // leading "ffmpeg" stripped
        assert_eq!(r.argv[1], r"C:\My Videos\clip one.mkv");
        assert_eq!(r.argv.last().unwrap(), r"C:\My Videos\clip one_wa.mp4");
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn resolve_without_output_warns() {
        let r = resolve(&cmd("-i {input} {dir}\\{name}_%03d.png"), r"C:\v\a.mkv").unwrap();
        assert_eq!(r.argv[2], r"C:\v\a_%03d.png");
        assert!(r.output_path.is_none());
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn validate_rejects_missing_input() {
        assert!(validate(&cmd("-c copy {output}")).is_err());
    }

    #[test]
    fn markdown_round_trip() {
        let mut a = cmd(r#"-i {input} -vf "scale=-2:720" -c:a aac {output}"#);
        a.label = "Convert to WhatsApp video".into();
        let mut b = cmd("-i {input} -vn -c:a libmp3lame -q:a 2 {output}");
        b.category = FileCategory::Audio;
        b.label = "To MP3".into();
        b.output_ext = "mp3".into();
        b.enabled = false;
        b.group = "Extract audio".into();
        let md = render_markdown(&[a.clone(), b.clone()]);
        let parsed = parse_markdown(&md);
        assert_eq!(parsed.len(), 2);
        for (orig, got) in [a, b].iter().zip(parsed.iter()) {
            assert_eq!(
                (&orig.category, &orig.label, &orig.args, &orig.output_name, &orig.output_ext, orig.enabled, &orig.group),
                (&got.category, &got.label, &got.args, &got.output_name, &got.output_ext, got.enabled, &got.group)
            );
        }
    }

    #[test]
    fn markdown_ignores_notes() {
        let md = "# x\nsome note\n## Images\n### Thumb\nblah\n- Output: `{name}_t.jpg`\n```ffmpeg\n-i {input}\n  -frames:v 1 {output}\n```\n";
        let p = parse_markdown(md);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].category, FileCategory::Image);
        assert_eq!(p[0].output_ext, "jpg");
        assert_eq!(p[0].args, "-i {input}\n  -frames:v 1 {output}");
    }
}
