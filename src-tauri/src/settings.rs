use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub const STORE_FILENAME: &str = "settings.json";
const SETTINGS_KEY: &str = "app_settings";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    Popup,
    MainWindow,
    Background,
}

impl Default for LaunchMode {
    fn default() -> Self {
        LaunchMode::Popup
    }
}

// `serde(default)`: a store written by an older build (missing new fields, or
// carrying removed ones like `crf`) still deserializes instead of silently
// resetting every setting to defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub custom_ffmpeg_path: Option<String>,
    pub launch_mode: LaunchMode,
    pub popup_auto_close: bool,
    pub notify_on_finish: bool,
    pub start_with_windows: bool,
    /// Owned by `set_context_menu_enabled`; `set_settings` never changes it.
    pub context_menu_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            custom_ffmpeg_path: None,
            launch_mode: LaunchMode::Popup,
            popup_auto_close: true,
            notify_on_finish: true,
            start_with_windows: false,
            context_menu_enabled: false,
        }
    }
}

pub fn load_settings(app: &AppHandle) -> AppSettings {
    if let Ok(store) = app.store(STORE_FILENAME) {
        if let Some(val) = store.get(SETTINGS_KEY) {
            if let Ok(settings) = serde_json::from_value::<AppSettings>(val) {
                return settings;
            }
        }
    }
    AppSettings::default()
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let store = app
        .store(STORE_FILENAME)
        .map_err(|e| format!("Failed to open settings store: {}", e))?;
    let val = serde_json::to_value(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    store.set(SETTINGS_KEY, val);
    store
        .save()
        .map_err(|e| format!("Failed to save settings store to disk: {}", e))?;
    apply_start_with_windows(settings.start_with_windows);
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_start_with_windows(enable: bool) {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((run_key, _)) =
        hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
    {
        if enable {
            if let Ok(exe) = std::env::current_exe() {
                let val = format!("\"{}\"", exe.to_string_lossy());
                let _ = run_key.set_value("FFMPEG-Toolkit", &val);
            }
        } else {
            let _ = run_key.delete_value("FFMPEG-Toolkit");
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_start_with_windows(_enable: bool) {}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    Ok(load_settings(&app))
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let mut settings = settings;
    // The frontend's copy may be stale for this flag (toggled via a separate
    // command), so the stored value always wins.
    settings.context_menu_enabled = load_settings(&app).context_menu_enabled;
    save_settings(&app, &settings)?;
    Ok(settings)
}
