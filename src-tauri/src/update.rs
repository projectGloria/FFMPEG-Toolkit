use crate::ffmpeg::detect_ffmpeg_internal;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::Duration;
use tauri::AppHandle;

const GYAN_RELEASE_URL: &str = "https://www.gyan.dev/ffmpeg/builds/release-version";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub is_git_build: bool,
    pub update_available: bool,
    pub status_message: String,
    pub error: Option<String>,
}

fn compare_numeric_versions(v1: &str, v2: &str) -> Ordering {
    let p1: Vec<u64> = v1
        .split('.')
        .map(|s| s.parse::<u64>().unwrap_or(0))
        .collect();
    let p2: Vec<u64> = v2
        .split('.')
        .map(|s| s.parse::<u64>().unwrap_or(0))
        .collect();

    let max_len = p1.len().max(p2.len());
    for i in 0..max_len {
        let a = *p1.get(i).unwrap_or(&0);
        let b = *p2.get(i).unwrap_or(&0);
        match a.cmp(&b) {
            Ordering::Equal => continue,
            ord => return ord,
        }
    }
    Ordering::Equal
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateCheckResult, String> {
    let ffmpeg_status = detect_ffmpeg_internal(&app).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("FFMPEG-Toolkit/0.1.0")
        .build()
        .map_err(|e| format!("Failed to initialize HTTP client: {}", e))?;

    let response = match client.get(GYAN_RELEASE_URL).send().await {
        Ok(resp) => resp,
        Err(err) => {
            let friendly = if err.is_timeout() {
                "Update check timed out while contacting gyan.dev. Please check your internet connection.".to_string()
            } else if err.is_connect() {
                "Could not connect to gyan.dev (you appear to be offline).".to_string()
            } else {
                format!("Network error while checking for updates: {}", err)
            };
            return Ok(UpdateCheckResult {
                installed_version: ffmpeg_status.version,
                latest_version: None,
                is_git_build: ffmpeg_status.is_git_build,
                update_available: false,
                status_message: "Unable to check for updates".to_string(),
                error: Some(friendly),
            });
        }
    };

    if !response.status().is_success() {
        return Ok(UpdateCheckResult {
            installed_version: ffmpeg_status.version,
            latest_version: None,
            is_git_build: ffmpeg_status.is_git_build,
            update_available: false,
            status_message: "Update server error".to_string(),
            error: Some(format!(
                "gyan.dev returned HTTP status {}",
                response.status()
            )),
        });
    }

    let latest_raw = match response.text().await {
        Ok(txt) => txt.trim().to_string(),
        Err(e) => {
            return Ok(UpdateCheckResult {
                installed_version: ffmpeg_status.version,
                latest_version: None,
                is_git_build: ffmpeg_status.is_git_build,
                update_available: false,
                status_message: "Failed to read update response".to_string(),
                error: Some(format!("Failed to read response text: {}", e)),
            });
        }
    };

    if !ffmpeg_status.installed {
        return Ok(UpdateCheckResult {
            installed_version: None,
            latest_version: Some(latest_raw.clone()),
            is_git_build: false,
            update_available: true,
            status_message: format!("FFmpeg is not installed (latest release: {})", latest_raw),
            error: None,
        });
    }

    if ffmpeg_status.is_git_build {
        return Ok(UpdateCheckResult {
            installed_version: ffmpeg_status.version,
            latest_version: Some(latest_raw),
            is_git_build: true,
            update_available: false,
            status_message: "Git build, can't compare".to_string(),
            error: None,
        });
    }

    let installed_ver = ffmpeg_status.version.unwrap_or_else(|| "0.0".to_string());
    match compare_numeric_versions(&installed_ver, &latest_raw) {
        Ordering::Less => Ok(UpdateCheckResult {
            installed_version: Some(installed_ver.clone()),
            latest_version: Some(latest_raw.clone()),
            is_git_build: false,
            update_available: true,
            status_message: format!("Update available: {} → {}", installed_ver, latest_raw),
            error: None,
        }),
        Ordering::Equal | Ordering::Greater => Ok(UpdateCheckResult {
            installed_version: Some(installed_ver),
            latest_version: Some(latest_raw),
            is_git_build: false,
            update_available: false,
            status_message: "Up to date".to_string(),
            error: None,
        }),
    }
}
