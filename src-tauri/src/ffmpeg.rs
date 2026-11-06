use crate::settings::load_settings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub raw_first_line: Option<String>,
    pub is_git_build: bool,
    pub executable_path: Option<String>,
    pub ffprobe_available: bool,
    pub ffprobe_path: Option<String>,
    pub source: Option<String>, // "custom" | "path"
    pub winget_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallLogEvent {
    pub line: String,
    pub is_done: bool,
    pub success: Option<bool>,
}

/// Parses the version token from the first line of `ffmpeg -version`.
/// Examples:
/// - `ffmpeg version 7.1-full_build-www.gyan.dev Copyright (c) ...` -> ("7.1", false)
/// - `ffmpeg version 7.0.2 Copyright ...` -> ("7.0.2", false)
/// - `ffmpeg version N-117452-gabcdef1234 Copyright ...` -> ("N-117452-gabcdef1234", true)
/// - `ffmpeg version 2024-11-03-git-abcdef-full_build ...` -> ("2024-11-03-git-abcdef", true)
/// - `ffmpeg version n7.1-3-g123456 ...` -> ("7.1", false) or git
pub fn parse_ffmpeg_version(first_line: &str) -> (String, bool) {
    let trimmed = first_line.trim();
    let raw_token = if let Some(after) = trimmed.strip_prefix("ffmpeg version ") {
        after.split_whitespace().next().unwrap_or("unknown")
    } else {
        trimmed.split_whitespace().nth(2).unwrap_or("unknown")
    };

    let lower = raw_token.to_lowercase();
    if raw_token.starts_with("N-")
        || raw_token.starts_with("git-")
        || lower.contains("-git")
        || lower.starts_with("git")
    {
        return (raw_token.to_string(), true);
    }

    // Strip common Gyan/BtbN suffixes such as `-full_build...` or `-essentials_build...`
    let base = raw_token
        .split('-')
        .next()
        .unwrap_or(raw_token)
        .trim_start_matches('n')
        .trim_start_matches('v');

    // Check if `base` looks like a standard numeric release version (e.g., 7.1 or 7.0.2)
    let is_numeric_release = !base.is_empty()
        && base.chars().all(|c| c.is_ascii_digit() || c == '.')
        && base.chars().any(|c| c.is_ascii_digit());

    // Note: YYYY-MM-DD date builds without dots are caught by `-` split above (e.g. "2024")
    if is_numeric_release && base.contains('.') {
        (base.to_string(), false)
    } else {
        (raw_token.to_string(), true)
    }
}

async fn resolve_executable_in_path(exe_name: &str) -> Option<String> {
    let mut cmd = Command::new("where.exe");
    cmd.arg(exe_name);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    if let Ok(output) = cmd.output().await {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(first) = stdout.lines().next() {
                let p = first.trim();
                if !p.is_empty() {
                    return Some(p.to_string());
                }
            }
        }
    }
    None
}

async fn check_winget_available() -> bool {
    let mut cmd = Command::new("winget");
    cmd.arg("--version");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd.output().await.map(|o| o.status.success()).unwrap_or(false)
}

async fn probe_ffmpeg_binary(candidate: &Path) -> Option<(String, String, bool)> {
    let mut cmd = Command::new(candidate);
    cmd.arg("-version");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next()?.trim().to_string();
    let (version, is_git) = parse_ffmpeg_version(&first_line);
    Some((first_line, version, is_git))
}

async fn probe_ffprobe_binary(ffmpeg_path: Option<&Path>) -> (bool, Option<String>) {
    // 1. Check sibling directory of custom/resolved ffmpeg path first
    if let Some(fp) = ffmpeg_path {
        if let Some(parent) = fp.parent() {
            let sibling = parent.join("ffprobe.exe");
            let mut cmd = Command::new(&sibling);
            cmd.arg("-version");
            #[cfg(target_os = "windows")]
            cmd.creation_flags(CREATE_NO_WINDOW);

            if let Ok(out) = cmd.output().await {
                if out.status.success() {
                    return (true, Some(sibling.to_string_lossy().to_string()));
                }
            }
        }
    }

    // 2. Fallback to PATH
    let mut cmd = Command::new("ffprobe");
    cmd.arg("-version");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    if let Ok(out) = cmd.output().await {
        if out.status.success() {
            let resolved = resolve_executable_in_path("ffprobe")
                .await
                .unwrap_or_else(|| "ffprobe".to_string());
            return (true, Some(resolved));
        }
    }

    (false, None)
}

pub async fn detect_ffmpeg_internal(app: &AppHandle) -> FfmpegStatus {
    let settings = load_settings(app);
    let winget_available = check_winget_available().await;

    // (a) Check user-configured custom path if configured and valid
    if let Some(ref custom_str) = settings.custom_ffmpeg_path {
        let trimmed = custom_str.trim();
        if !trimmed.is_empty() {
            let candidate_path = {
                let p = PathBuf::from(trimmed);
                if p.is_dir() {
                    p.join("ffmpeg.exe")
                } else {
                    p
                }
            };
            if let Some((first_line, version, is_git)) = probe_ffmpeg_binary(&candidate_path).await
            {
                let (ffprobe_available, ffprobe_path) =
                    probe_ffprobe_binary(Some(&candidate_path)).await;
                return FfmpegStatus {
                    installed: true,
                    version: Some(version),
                    raw_first_line: Some(first_line),
                    is_git_build: is_git,
                    executable_path: Some(candidate_path.to_string_lossy().to_string()),
                    ffprobe_available,
                    ffprobe_path,
                    source: Some("custom".to_string()),
                    winget_available,
                };
            }
        }
    }

    // (b) Check `ffmpeg -version` from PATH
    if let Some((first_line, version, is_git)) = probe_ffmpeg_binary(Path::new("ffmpeg")).await {
        let resolved_path = resolve_executable_in_path("ffmpeg")
            .await
            .unwrap_or_else(|| "ffmpeg".to_string());
        let path_buf = PathBuf::from(&resolved_path);
        let (ffprobe_available, ffprobe_path) = probe_ffprobe_binary(Some(&path_buf)).await;

        return FfmpegStatus {
            installed: true,
            version: Some(version),
            raw_first_line: Some(first_line),
            is_git_build: is_git,
            executable_path: Some(resolved_path),
            ffprobe_available,
            ffprobe_path,
            source: Some("path".to_string()),
            winget_available,
        };
    }

    FfmpegStatus {
        installed: false,
        version: None,
        raw_first_line: None,
        is_git_build: false,
        executable_path: None,
        ffprobe_available: false,
        ffprobe_path: None,
        source: None,
        winget_available,
    }
}

#[tauri::command]
pub async fn check_ffmpeg(app: AppHandle) -> Result<FfmpegStatus, String> {
    Ok(detect_ffmpeg_internal(&app).await)
}

#[tauri::command]
pub async fn install_or_update_ffmpeg(app: AppHandle, is_update: bool) -> Result<bool, String> {
    if !check_winget_available().await {
        return Err(
            "WinGet is not available on this system. Please download FFmpeg from gyan.dev."
                .to_string(),
        );
    }

    let args: Vec<&str> = if is_update {
        vec![
            "upgrade",
            "--id",
            "Gyan.FFmpeg",
            "-e",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ]
    } else {
        vec![
            "install",
            "--id",
            "Gyan.FFmpeg",
            "-e",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ]
    };

    let _ = app.emit(
        "ffmpeg-install-log",
        InstallLogEvent {
            line: format!("> winget {}", args.join(" ")),
            is_done: false,
            success: None,
        },
    );

    let mut cmd = Command::new("winget");
    cmd.args(&args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to launch winget: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_out = app.clone();
    let out_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let cleaned = line.trim().to_string();
                if !cleaned.is_empty() {
                    let _ = app_out.emit(
                        "ffmpeg-install-log",
                        InstallLogEvent {
                            line: cleaned,
                            is_done: false,
                            success: None,
                        },
                    );
                }
            }
        }
    });

    let app_err = app.clone();
    let err_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let cleaned = line.trim().to_string();
                if !cleaned.is_empty() {
                    let _ = app_err.emit(
                        "ffmpeg-install-log",
                        InstallLogEvent {
                            line: cleaned,
                            is_done: false,
                            success: None,
                        },
                    );
                }
            }
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed while waiting for winget: {}", e))?;

    let _ = out_task.await;
    let _ = err_task.await;

    let ok = status.success();
    let msg = if ok {
        "WinGet operation completed successfully. Note: If FFmpeg was just installed, you may need to restart the app for PATH changes to take effect."
    } else {
        "WinGet operation exited with a non-zero status code."
    };

    let _ = app.emit(
        "ffmpeg-install-log",
        InstallLogEvent {
            line: msg.to_string(),
            is_done: true,
            success: Some(ok),
        },
    );

    Ok(ok)
}
