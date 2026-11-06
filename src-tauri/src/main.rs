#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod commands;
mod context_menu;
mod ffmpeg;
mod presets;
mod settings;
mod update;

use actions::{
    cancel_job, clear_finished_jobs, enqueue_and_run, get_jobs, get_missing_ffmpeg_alert,
    open_main_window_from_popup, run_command, AppJobQueue,
};
use commands::{
    add_presets, delete_command, export_commands_md, get_categories, get_commands,
    import_commands_md, move_command, preview_command, save_command, seed_presets_on_first_run,
};
use context_menu::{context_menu_status, set_context_menu_enabled, sync_on_startup};
use ffmpeg::{check_ffmpeg, install_or_update_ffmpeg};
use settings::{get_settings, load_settings, set_settings, LaunchMode};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};
use update::check_update;

/// Parses `--cmd <command-id> <file1> [file2 ...]` (also `--cmd=<id>`), which is
/// what the registry entries written by `context_menu.rs` invoke.
fn parse_cli_args(args: &[String]) -> Option<(String, Vec<String>)> {
    let mut command_id: Option<String> = None;
    let mut files: Vec<String> = Vec::new();

    let mut i = 1; // skip executable path at index 0
    while i < args.len() {
        let arg = args[i].trim();
        if arg == "--cmd" {
            command_id = args.get(i + 1).map(|s| s.trim().to_string());
            i += 2;
            continue;
        } else if let Some(val) = arg.strip_prefix("--cmd=") {
            command_id = Some(val.trim().to_string());
        } else if !arg.starts_with("--") && !arg.is_empty() {
            files.push(arg.to_string());
        }
        i += 1;
    }

    match command_id {
        Some(id) if !id.is_empty() && !files.is_empty() => Some((id, files)),
        _ => None,
    }
}

fn setup_system_tray(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, "open_main", "Open FFMPEG-Toolkit", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit_app", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    let mut tray_builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("FFMPEG-Toolkit")
        .show_menu_on_left_click(false)
        .on_menu_event(|app_handle, event| match event.id.as_ref() {
            "open_main" => {
                let qs = app_handle.state::<Arc<AppJobQueue>>();
                qs.main_window_opened_manually
                    .store(true, Ordering::SeqCst);
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "quit_app" => {
                app_handle.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app_handle = tray.app_handle();
                let qs = app_handle.state::<Arc<AppJobQueue>>();
                qs.main_window_opened_manually
                    .store(true, Ordering::SeqCst);
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        });

    // Load tray icon from the bundled icon (`app.default_window_icon()`), not from a separate file
    if let Some(default_icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(default_icon.clone());
    }

    let _ = tray_builder.build(app)?;
    Ok(())
}

fn main() {
    let job_queue = Arc::new(AppJobQueue::new());

    tauri::Builder::default()
        .manage(job_queue)
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // When user selects multiple files in Explorer and clicks a context menu action,
            // Windows invokes the command per file. `single-instance` forwards argv here.
            if let Some((command_id, files)) = parse_cli_args(&argv) {
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = enqueue_and_run(app_clone, command_id, files, true).await;
                });
            } else {
                // Launched normally while already running: bring main window to front
                let qs = app.state::<Arc<AppJobQueue>>();
                qs.main_window_opened_manually
                    .store(true, Ordering::SeqCst);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // 0. Brand-new install: populate the built-in presets (no-op afterwards).
            seed_presets_on_first_run(&app_handle);

            // 1. Re-sync the Explorer menu from stored commands: repairs a moved exe
            //    and wipes legacy `--action` entries. Store plugin is already registered.
            sync_on_startup(&app_handle);

            // 2. Initialize system tray icon from app.default_window_icon()
            let _ = setup_system_tray(&app_handle);

            // 3. Intercept window close on popup so it hides instead of destroying the webview,
            //    and on main window if background jobs are still running.
            if let Some(popup_win) = app.get_webview_window("popup") {
                let p_clone = popup_win.clone();
                let app_for_popup = app_handle.clone();
                popup_win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = p_clone.hide();
                        let qs = app_for_popup.state::<Arc<AppJobQueue>>();
                        if !qs.worker_running.load(Ordering::SeqCst)
                            && !qs.main_window_opened_manually.load(Ordering::SeqCst)
                        {
                            app_for_popup.exit(0);
                        }
                    }
                });
            }

            if let Some(main_win) = app.get_webview_window("main") {
                let m_clone = main_win.clone();
                let app_for_main = app_handle.clone();
                main_win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        let qs = app_for_main.state::<Arc<AppJobQueue>>();
                        if qs.worker_running.load(Ordering::SeqCst) {
                            // Keep alive in tray while jobs are active
                            api.prevent_close();
                            let _ = m_clone.hide();
                        } else {
                            app_for_main.exit(0);
                        }
                    }
                });
            }

            // 4. Read launch preference from tauri-plugin-store before showing any window
            let settings = load_settings(&app_handle);
            let cli_args: Vec<String> = std::env::args().collect();

            if let Some((command_id, files)) = parse_cli_args(&cli_args) {
                match settings.launch_mode {
                    LaunchMode::Popup => {
                        if let Some(w) = app.get_webview_window("popup") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    LaunchMode::MainWindow => {
                        let qs = app.state::<Arc<AppJobQueue>>();
                        qs.main_window_opened_manually
                            .store(true, Ordering::SeqCst);
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                        let _ = app_handle.emit("navigate-tab", "queue");
                    }
                    LaunchMode::Background => {
                        let qs = app.state::<Arc<AppJobQueue>>();
                        qs.started_in_background_mode
                            .store(true, Ordering::SeqCst);
                        // Do not show any window
                    }
                }

                let app_clone = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = enqueue_and_run(app_clone, command_id, files, true).await;
                });
            } else {
                // Normal launch (no --cmd): show the main window
                let qs = app.state::<Arc<AppJobQueue>>();
                qs.main_window_opened_manually
                    .store(true, Ordering::SeqCst);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_ffmpeg,
            check_update,
            install_or_update_ffmpeg,
            set_context_menu_enabled,
            context_menu_status,
            get_categories,
            get_commands,
            save_command,
            delete_command,
            move_command,
            preview_command,
            export_commands_md,
            import_commands_md,
            add_presets,
            run_command,
            cancel_job,
            get_jobs,
            clear_finished_jobs,
            get_missing_ffmpeg_alert,
            open_main_window_from_popup,
            get_settings,
            set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FFMPEG-Toolkit");
}
