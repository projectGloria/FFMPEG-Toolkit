//! Explorer context menu, generated from the user's commands.
//! Strategy is a full rewrite on every change (delete our keys under every
//! target, then re-create): a few hundred tiny HKCU writes, and it means stale
//! entries for deleted/disabled commands can never linger.

use crate::commands::{load_commands, CustomCommand, FileCategory};
use crate::settings::{load_settings, save_settings};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use winreg::enums::*;
use winreg::RegKey;

/// Whether sub-entries also show the exe icon (the parent always does).
pub const SHOW_SUBENTRY_ICONS: bool = false;

const MENU_KEY_NAME: &str = "FFMPEGToolkit";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMenuStatus {
    pub enabled: bool,
    pub registered_exe_path: Option<String>,
    pub current_exe_path: String,
    pub path_matches: bool,
    /// Number of enabled commands currently written to the registry.
    pub registered_commands: usize,
}

fn current_exe_string() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("Failed to resolve current executable path: {}", e))
}

fn shell_path(target: &str) -> String {
    format!("Software\\Classes\\SystemFileAssociations\\{}\\shell", target)
}

/// Every top-level key we own under a target's `shell` (`FFMPEGToolkit`,
/// `FFMPEGToolkit_00`, ...). Prefix match also sweeps keys from older builds.
fn our_root_keys(hkcu: &RegKey, target: &str) -> Vec<String> {
    let Ok(shell) = hkcu.open_subkey(shell_path(target)) else {
        return Vec::new();
    };
    shell
        .enum_keys()
        .filter_map(Result::ok)
        .filter(|k| k.starts_with(MENU_KEY_NAME))
        .map(|k| format!("{}\\{}", shell_path(target), k))
        .collect()
}

fn remove_all(hkcu: &RegKey) -> Result<(), String> {
    for cat in FileCategory::ALL {
        for target in cat.registry_targets() {
            for key in our_root_keys(hkcu, target) {
                hkcu.delete_subkey_all(&key)
                    .map_err(|e| format!("Failed to remove registry key {}: {}", key, e))?;
            }
        }
    }
    Ok(())
}

/// Explorer shows at most 16 static (registry) verbs per top-level cascading
/// menu, counted across ALL its nesting levels; the rest are dropped silently.
/// Verified by enumerating the real shell menu: nested `SubCommands` submenus
/// and `ExtendedSubCommandsKey` share the same budget, but separate top-level
/// entries each get their own. So each submenu becomes its own top-level entry.
const MAX_PER_ROOT: usize = 16;

#[derive(Debug, PartialEq)]
struct MenuRoot<'a> {
    label: String,
    cmds: Vec<&'a CustomCommand>,
}

/// One root per submenu name (in order of first appearance, case-insensitive),
/// ungrouped commands under the plain "FFMPEG-Toolkit" root. Roots over the cap
/// are split into "… (2)", "… (3)" so no configuration can lose an entry.
fn menu_roots<'a>(cmds: &[&'a CustomCommand]) -> Vec<MenuRoot<'a>> {
    let mut groups: Vec<(String, Vec<&'a CustomCommand>)> = Vec::new();
    for &c in cmds {
        match groups.iter_mut().find(|(g, _)| g.eq_ignore_ascii_case(&c.group)) {
            Some((_, list)) => list.push(c),
            None => groups.push((c.group.clone(), vec![c])),
        }
    }
    let mut roots = Vec::new();
    for (group, list) in groups {
        let base = if group.is_empty() { "FFMPEG-Toolkit".to_string() } else { format!("FFMPEG · {}", group) };
        for (i, chunk) in list.chunks(MAX_PER_ROOT).enumerate() {
            let label = if i == 0 { base.clone() } else { format!("{} ({})", base, i + 1) };
            roots.push(MenuRoot { label, cmds: chunk.to_vec() });
        }
    }
    roots
}

fn write_target(hkcu: &RegKey, target: &str, exe: &str, roots: &[MenuRoot]) -> Result<(), String> {
    let icon = format!("\"{}\",0", exe);
    let err = |what: &str, e: std::io::Error| format!("Registry write failed ({}): {}", what, e);

    for (r, root) in roots.iter().enumerate() {
        // Top-level verbs are ordered by key name too, so the index keeps roots in order.
        let base = format!("{}\\{}_{:02}", shell_path(target), MENU_KEY_NAME, r);
        let (menu, _) = hkcu.create_subkey(&base).map_err(|e| err(&base, e))?;
        menu.set_value("MUIVerb", &root.label).map_err(|e| err("MUIVerb", e))?;
        // Empty `SubCommands` + a nested `shell` key = cascading menu.
        menu.set_value("SubCommands", &"").map_err(|e| err("SubCommands", e))?;
        menu.set_value("Icon", &icon).map_err(|e| err("Icon", e))?;
        menu.set_value("ExePath", &exe).map_err(|e| err("ExePath", e))?;

        for (i, c) in root.cmds.iter().enumerate() {
            // Explorer sorts sub-verbs by key name, so the NN prefix carries the order.
            let id8: String = c.id.chars().filter(|ch| ch.is_ascii_alphanumeric()).take(8).collect();
            let sub = format!("{}\\shell\\{:02}_{}", base, i, id8);
            let (sub_key, _) = hkcu.create_subkey(&sub).map_err(|e| err(&sub, e))?;
            sub_key.set_value("MUIVerb", &c.label).map_err(|e| err("MUIVerb", e))?;
            if SHOW_SUBENTRY_ICONS {
                let _ = sub_key.set_value("Icon", &icon);
            }
            let (cmd_key, _) = hkcu
                .create_subkey(format!("{}\\command", sub))
                .map_err(|e| err("command", e))?;
            let line = format!("\"{}\" --cmd {} \"%1\"", exe, c.id);
            cmd_key.set_value("", &line).map_err(|e| err("command", e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::presets_for;

    #[test]
    fn video_presets_become_one_root_per_submenu() {
        let cmds = presets_for(FileCategory::Video);
        let refs: Vec<_> = cmds.iter().collect();
        let roots = menu_roots(&refs);
        let labels: Vec<_> = roots.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["FFMPEG · Convert video", "FFMPEG · Extract audio", "FFMPEG · Extract subtitles"]);
        assert!(roots.iter().all(|r| r.cmds.len() <= MAX_PER_ROOT));
        assert_eq!(roots.iter().map(|r| r.cmds.len()).sum::<usize>(), cmds.len());
    }

    #[test]
    fn oversized_root_is_split_without_losing_entries() {
        let mut cmds = presets_for(FileCategory::Video);
        cmds.extend(presets_for(FileCategory::Video));
        cmds.iter_mut().for_each(|c| c.group.clear()); // 50 ungrouped
        let refs: Vec<_> = cmds.iter().collect();
        let roots = menu_roots(&refs);
        let labels: Vec<_> = roots.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["FFMPEG-Toolkit", "FFMPEG-Toolkit (2)", "FFMPEG-Toolkit (3)", "FFMPEG-Toolkit (4)"]);
        assert!(roots.iter().all(|r| r.cmds.len() <= MAX_PER_ROOT));
        assert_eq!(roots.iter().map(|r| r.cmds.len()).sum::<usize>(), cmds.len());
    }
}

/// Rewrites (or removes, when disabled) every registry entry from `cmds`.
pub fn sync_context_menu(app: &AppHandle, cmds: &[CustomCommand]) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    remove_all(&hkcu)?;
    if !load_settings(app).context_menu_enabled {
        return Ok(());
    }
    let exe = current_exe_string()?;
    for cat in FileCategory::ALL {
        let in_cat: Vec<&CustomCommand> =
            cmds.iter().filter(|c| c.category == cat && c.enabled).collect();
        if in_cat.is_empty() {
            continue; // no empty "FFMPEG-Toolkit ▶" submenus
        }
        let roots = menu_roots(&in_cat);
        for target in cat.registry_targets() {
            write_target(&hkcu, target, &exe, &roots)?;
        }
    }
    Ok(())
}

pub fn get_status(app: &AppHandle) -> Result<ContextMenuStatus, String> {
    let current_exe = current_exe_string()?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // First category with a registered key tells us which exe the menu points at.
    let registered_exe_path = FileCategory::ALL.iter().find_map(|cat| {
        let root = our_root_keys(&hkcu, cat.registry_targets()[0]).into_iter().next()?;
        hkcu.open_subkey(root).ok()?.get_value::<String, _>("ExePath").ok()
    });
    let path_matches = registered_exe_path
        .as_ref()
        .map(|s| s.eq_ignore_ascii_case(&current_exe))
        .unwrap_or(false);
    let enabled = load_settings(app).context_menu_enabled;
    let registered_commands = if enabled {
        load_commands(app).iter().filter(|c| c.enabled).count()
    } else {
        0
    };

    Ok(ContextMenuStatus {
        enabled,
        registered_exe_path,
        current_exe_path: current_exe,
        path_matches,
        registered_commands,
    })
}

/// Startup: always re-sync. Repairs a moved/updated exe, and also wipes legacy
/// `--action` entries from pre-custom-command builds when the menu is disabled.
pub fn sync_on_startup(app: &AppHandle) {
    let _ = sync_context_menu(app, &load_commands(app));
}

fn set_enabled(app: &AppHandle, enabled: bool) -> Result<ContextMenuStatus, String> {
    let mut s = load_settings(app);
    s.context_menu_enabled = enabled;
    save_settings(app, &s)?;
    sync_context_menu(app, &load_commands(app))?;
    get_status(app)
}

#[tauri::command]
pub fn context_menu_status(app: AppHandle) -> Result<ContextMenuStatus, String> {
    get_status(&app)
}

#[tauri::command]
pub fn set_context_menu_enabled(app: AppHandle, enabled: bool) -> Result<ContextMenuStatus, String> {
    set_enabled(&app, enabled)
}
