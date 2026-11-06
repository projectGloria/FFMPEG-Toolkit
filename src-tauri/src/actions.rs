use crate::commands::{display_argv, load_commands, resolve, CustomCommand, FileCategory};
use crate::ffmpeg::{detect_ffmpeg_internal, CREATE_NO_WINDOW};
use crate::settings::{load_settings, LaunchMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{watch, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobItem {
    pub id: String,
    /// Snapshot taken at enqueue time: editing/deleting the command later must
    /// not change what an already-queued job runs.
    pub command: CustomCommand,
    pub input_path: String,
    pub file_name: String,
    pub folder_path: String,
    pub output_files: Vec<String>,
    pub state: JobState,
    pub progress: f64, // 0.0..=100.0
    /// False when the input has no duration (still images) → UI shows an indeterminate bar.
    pub has_progress: bool,
    pub status_detail: String,
    pub error: Option<String>,
    /// Resolved command line, for debugging failed jobs.
    pub command_line: Option<String>,
}

pub struct AppJobQueue {
    pub jobs: Mutex<Vec<JobItem>>,
    pub cancel_tokens: Mutex<HashMap<String, watch::Sender<bool>>>,
    pub worker_running: AtomicBool,
    pub main_window_opened_manually: AtomicBool,
    pub started_in_background_mode: AtomicBool,
    pub missing_ffmpeg_alert: Mutex<Option<String>>,
}

impl AppJobQueue {
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(Vec::new()),
            cancel_tokens: Mutex::new(HashMap::new()),
            worker_running: AtomicBool::new(false),
            main_window_opened_manually: AtomicBool::new(false),
            started_in_background_mode: AtomicBool::new(false),
            missing_ffmpeg_alert: Mutex::new(None),
        }
    }
}

fn show_window(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Generates a unique output file path without overwriting.
/// E.g., `<dir>/<stem>.<ext>` -> `<dir>/<stem> (1).<ext>` -> `<dir>/<stem> (2).<ext>`
pub fn next_available_path(dir: &Path, stem: &str, ext_without_dot: &str) -> PathBuf {
    let candidate = dir.join(format!("{}.{}", stem, ext_without_dot));
    if !candidate.exists() {
        return candidate;
    }
    let mut counter = 1u32;
    loop {
        let numbered = dir.join(format!("{} ({}).{}", stem, counter, ext_without_dot));
        if !numbered.exists() {
            return numbered;
        }
        counter += 1;
    }
}

#[derive(Debug, Deserialize)]
struct FfprobeFormatOnly {
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

/// Duration in seconds, used as the denominator for `out_time_us` progress.
async fn probe_duration(ffprobe_bin: &str, input_path: &str) -> Option<f64> {
    let mut cmd = Command::new(ffprobe_bin);
    cmd.args(["-v", "quiet", "-print_format", "json", "-show_format", input_path]);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().await.ok().filter(|o| o.status.success())?;
    serde_json::from_slice::<FfprobeFormatOnly>(&output.stdout)
        .ok()?
        .format?
        .duration?
        .parse::<f64>()
        .ok()
        .filter(|&d| d > 0.0)
}

async fn emit_jobs_state(app: &AppHandle) {
    let state = app.state::<Arc<AppJobQueue>>();
    let jobs = state.jobs.lock().await.clone();
    let _ = app.emit("jobs-updated", &jobs);
}

async fn update_job<F>(app: &AppHandle, job_id: &str, updater: F)
where
    F: FnOnce(&mut JobItem),
{
    let state = app.state::<Arc<AppJobQueue>>();
    {
        let mut jobs = state.jobs.lock().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            updater(job);
        }
    }
    emit_jobs_state(app).await;
}

async fn fail_job(app: &AppHandle, job_id: &str, detail: &str, error: String) {
    update_job(app, job_id, |j| {
        j.state = JobState::Failed;
        j.status_detail = detail.to_string();
        j.error = Some(error);
    })
    .await;
}

fn send_toast(app: &AppHandle, title: &str, body: &str) {
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

pub fn show_main_window_install_screen(app: &AppHandle, reason: Option<String>) {
    let state = app.state::<Arc<AppJobQueue>>();
    state
        .main_window_opened_manually
        .store(true, Ordering::SeqCst);
    show_window(app, "main");
    let _ = app.emit("navigate-tab", "ffmpeg");
    if let Some(msg) = reason {
        let _ = app.emit("missing-ffmpeg-error", msg);
    }
}

/// Single entry point for UI runs and Explorer/CLI launches.
pub async fn enqueue_and_run(
    app: AppHandle,
    command_id: String,
    files: Vec<String>,
    from_cli: bool,
) -> Result<Vec<JobItem>, String> {
    let settings = load_settings(&app);
    let queue_state = app.state::<Arc<AppJobQueue>>().inner().clone();

    if from_cli {
        match settings.launch_mode {
            LaunchMode::Popup => show_window(&app, "popup"),
            LaunchMode::MainWindow => {
                queue_state
                    .main_window_opened_manually
                    .store(true, Ordering::SeqCst);
                show_window(&app, "main");
                let _ = app.emit("navigate-tab", "queue");
            }
            LaunchMode::Background => {
                queue_state
                    .started_in_background_mode
                    .store(true, Ordering::SeqCst);
            }
        }
    }

    // A stale registry entry can point at a deleted command. Instead of a silent
    // no-op, surface failed jobs so the user sees *why* nothing happened.
    let command = load_commands(&app).into_iter().find(|c| c.id == command_id);
    let unknown_err = command.is_none().then(|| {
        format!(
            "Command '{}' no longer exists. Re-enable the context menu in Settings to refresh Explorer entries.",
            command_id
        )
    });
    let command = command.unwrap_or_else(|| CustomCommand {
        id: command_id.clone(),
        category: FileCategory::Video,
        label: "Unknown command".into(),
        args: String::new(),
        output_name: "{name}".into(),
        output_ext: "mp4".into(),
        enabled: false,
        group: String::new(),
    });

    if unknown_err.is_none() {
        let ffmpeg_status = detect_ffmpeg_internal(&app).await;
        if !ffmpeg_status.installed {
            let err_msg = "FFmpeg is not installed or could not be found. Please install FFmpeg or configure its custom path.".to_string();
            *queue_state.missing_ffmpeg_alert.lock().await = Some(err_msg.clone());
            let _ = app.emit("missing-ffmpeg-error", err_msg.clone());

            if from_cli {
                match settings.launch_mode {
                    LaunchMode::Background => {
                        send_toast(
                            &app,
                            "FFMPEG-Toolkit: FFmpeg Missing",
                            "FFmpeg is not installed. Open FFMPEG-Toolkit to install it.",
                        );
                        show_main_window_install_screen(&app, Some(err_msg.clone()));
                    }
                    LaunchMode::Popup => show_window(&app, "popup"),
                    LaunchMode::MainWindow => {
                        show_main_window_install_screen(&app, Some(err_msg.clone()))
                    }
                }
            }
            return Err(err_msg);
        }
        *queue_state.missing_ffmpeg_alert.lock().await = None;
        let _ = app.emit("missing-ffmpeg-error", Option::<String>::None);
    }

    {
        let mut jobs = queue_state.jobs.lock().await;
        for raw_path in files {
            let trimmed = raw_path.trim().trim_matches('"').to_string();
            if trimmed.is_empty() {
                continue;
            }
            let path_obj = PathBuf::from(&trimmed);
            let file_name = path_obj
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| trimmed.clone());
            let folder_path = path_obj
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let (state, status_detail) = match &unknown_err {
                Some(_) => (JobState::Failed, "Unknown command".to_string()),
                None => (JobState::Queued, "Queued".to_string()),
            };
            jobs.push(JobItem {
                id: Uuid::new_v4().to_string(),
                command: command.clone(),
                input_path: trimmed,
                file_name,
                folder_path,
                output_files: Vec::new(),
                state,
                progress: 0.0,
                has_progress: true,
                status_detail,
                error: unknown_err.clone(),
                command_line: None,
            });
        }
    }

    emit_jobs_state(&app).await;

    if let Some(err) = unknown_err {
        send_toast(&app, "FFMPEG-Toolkit: Unknown command", &err);
        return Err(err);
    }

    // Exactly one sequential worker: `swap` is the claim.
    if !queue_state.worker_running.swap(true, Ordering::SeqCst) {
        let app_clone = app.clone();
        tokio::spawn(async move {
            process_queue_loop(app_clone).await;
        });
    }

    let current_jobs = queue_state.jobs.lock().await.clone();
    Ok(current_jobs)
}

async fn process_queue_loop(app: AppHandle) {
    let queue_state = app.state::<Arc<AppJobQueue>>().inner().clone();

    loop {
        let next_job = {
            let jobs = queue_state.jobs.lock().await;
            jobs.iter().find(|j| j.state == JobState::Queued).cloned()
        };
        let Some(job) = next_job else {
            break;
        };

        let (cancel_tx, cancel_rx) = watch::channel(false);
        queue_state
            .cancel_tokens
            .lock()
            .await
            .insert(job.id.clone(), cancel_tx);

        run_custom_command_job(&app, &job, cancel_rx).await;

        queue_state.cancel_tokens.lock().await.remove(&job.id);

        let updated_job = {
            let jobs = queue_state.jobs.lock().await;
            jobs.iter().find(|j| j.id == job.id).cloned()
        };

        if let Some(fin_job) = updated_job {
            let settings = load_settings(&app);
            let is_bg = settings.launch_mode == LaunchMode::Background
                && queue_state
                    .started_in_background_mode
                    .load(Ordering::SeqCst);

            match fin_job.state {
                JobState::Completed => {
                    if settings.notify_on_finish || is_bg {
                        let out_names: Vec<String> = fin_job
                            .output_files
                            .iter()
                            .filter_map(|p| {
                                Path::new(p)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                            })
                            .collect();
                        let summary = if out_names.is_empty() {
                            fin_job.status_detail.clone()
                        } else {
                            format!("Output: {}", out_names.join(", "))
                        };
                        send_toast(
                            &app,
                            &format!("{} — {}", fin_job.command.label, fin_job.file_name),
                            &summary,
                        );
                    }
                }
                JobState::Failed => {
                    let err_text = fin_job
                        .error
                        .clone()
                        .unwrap_or_else(|| "Unknown error occurred".to_string());
                    // Errors are always surfaced, even in background mode.
                    send_toast(
                        &app,
                        &format!("Failed: {} — {}", fin_job.command.label, fin_job.file_name),
                        &err_text,
                    );
                    if is_bg {
                        queue_state
                            .main_window_opened_manually
                            .store(true, Ordering::SeqCst);
                        show_window(&app, "main");
                        let _ = app.emit("navigate-tab", "queue");
                    }
                }
                _ => {}
            }
        }
    }

    queue_state.worker_running.store(false, Ordering::SeqCst);
    let _ = app.emit("queue-finished", true);

    let settings = load_settings(&app);
    let any_failed = {
        let jobs = queue_state.jobs.lock().await;
        jobs.iter().any(|j| j.state == JobState::Failed)
    };

    if settings.launch_mode == LaunchMode::Popup && settings.popup_auto_close && !any_failed {
        let app_for_popup = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
            let qs = app_for_popup.state::<Arc<AppJobQueue>>();
            // Re-check: a new Explorer invocation may have restarted the worker meanwhile.
            if !qs.worker_running.load(Ordering::SeqCst) {
                if let Some(popup_win) = app_for_popup.get_webview_window("popup") {
                    let _ = popup_win.hide();
                }
                if !qs.main_window_opened_manually.load(Ordering::SeqCst) {
                    app_for_popup.exit(0);
                }
            }
        });
    }

    if queue_state
        .started_in_background_mode
        .load(Ordering::SeqCst)
        && !queue_state
            .main_window_opened_manually
            .load(Ordering::SeqCst)
        && !any_failed
    {
        let app_for_exit = app.clone();
        tokio::spawn(async move {
            // Let the toast dispatch before the process dies.
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let qs = app_for_exit.state::<Arc<AppJobQueue>>();
            if !qs.worker_running.load(Ordering::SeqCst)
                && !qs.main_window_opened_manually.load(Ordering::SeqCst)
            {
                app_for_exit.exit(0);
            }
        });
    }
}

async fn run_custom_command_job(
    app: &AppHandle,
    job: &JobItem,
    mut cancel_rx: watch::Receiver<bool>,
) {
    if !Path::new(&job.input_path).exists() {
        fail_job(app, &job.id, "Failed", format!("Source file does not exist: {}", job.input_path)).await;
        return;
    }

    let ffmpeg_status = detect_ffmpeg_internal(app).await;
    let Some(ffmpeg_bin) = ffmpeg_status.executable_path else {
        fail_job(app, &job.id, "FFmpeg missing", "FFmpeg executable could not be found.".into()).await;
        return;
    };

    // Resolve right before running (not at enqueue): `next_available_path` must
    // see outputs produced by earlier jobs in the same batch.
    let resolved = match resolve(&job.command, &job.input_path) {
        Ok(r) => r,
        Err(e) => {
            fail_job(app, &job.id, "Invalid command", e).await;
            return;
        }
    };
    let output_path = resolved.output_path.as_ref().map(PathBuf::from);

    update_job(app, &job.id, |j| {
        j.state = JobState::Running;
        j.progress = 0.0;
        j.status_detail = "Probing duration...".to_string();
        j.command_line = Some(format!("ffmpeg {}", display_argv(&resolved.argv)));
    })
    .await;

    let total_duration = match ffmpeg_status.ffprobe_path.as_deref() {
        Some(probe) => probe_duration(probe, &job.input_path).await,
        None => None,
    };

    update_job(app, &job.id, |j| {
        j.has_progress = total_duration.is_some();
        j.status_detail = "Running ffmpeg...".to_string();
    })
    .await;

    let mut cmd = Command::new(&ffmpeg_bin);
    // Global options go before the user's args (ffmpeg accepts them anywhere
    // before the first output). `-nostdin` makes an unexpected overwrite prompt
    // fail fast instead of hanging forever on a pipe nobody writes to.
    cmd.args(["-hide_banner", "-nostdin", "-progress", "pipe:1", "-nostats"])
        .args(&resolved.argv);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            fail_job(app, &job.id, "Failed to spawn FFmpeg", format!("Failed to start FFmpeg process: {}", e)).await;
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Keep only the stderr tail: ffmpeg is chatty and the error is at the end.
    let stderr_handle = tokio::spawn(async move {
        let mut err_lines = Vec::new();
        if let Some(err) = stderr {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                err_lines.push(line);
                if err_lines.len() > 35 {
                    err_lines.remove(0);
                }
            }
        }
        err_lines.join("\n")
    });

    let app_for_prog = app.clone();
    let job_id_for_prog = job.id.clone();
    let stdout_handle = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                // ffmpeg's `out_time_ms` is actually microseconds too (historical bug).
                if let Some(us_str) = trimmed
                    .strip_prefix("out_time_us=")
                    .or_else(|| trimmed.strip_prefix("out_time_ms="))
                {
                    if let (Ok(us), Some(dur)) = (us_str.trim().parse::<f64>(), total_duration) {
                        if us >= 0.0 {
                            let pct = ((us / 1_000_000.0 / dur) * 100.0).clamp(0.0, 99.5);
                            update_job(&app_for_prog, &job_id_for_prog, |j| {
                                j.progress = (pct * 10.0).round() / 10.0;
                            })
                            .await;
                        }
                    }
                } else if trimmed == "progress=end" {
                    update_job(&app_for_prog, &job_id_for_prog, |j| j.progress = 100.0).await;
                }
            }
        }
    });

    let mut was_cancelled = false;
    let wait_res = tokio::select! {
        res = child.wait() => Some(res),
        _ = cancel_rx.changed() => {
            if *cancel_rx.borrow() {
                was_cancelled = true;
                let _ = child.kill().await;
            }
            None
        }
    };

    let _ = stdout_handle.await;
    let stderr_log = stderr_handle.await.unwrap_or_default();

    let remove_partial = || async {
        if let Some(p) = &output_path {
            if p.exists() {
                let _ = tokio::fs::remove_file(p).await;
            }
        }
    };

    if was_cancelled {
        remove_partial().await;
        update_job(app, &job.id, |j| {
            j.state = JobState::Cancelled;
            j.status_detail = "Cancelled by user".to_string();
        })
        .await;
        return;
    }

    match wait_res {
        Some(Ok(status)) if status.success() => {
            update_job(app, &job.id, |j| {
                j.state = JobState::Completed;
                j.progress = 100.0;
                j.has_progress = true;
                match &output_path {
                    Some(p) => {
                        j.output_files = vec![p.to_string_lossy().to_string()];
                        j.status_detail = format!(
                            "Completed → {}",
                            p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                        );
                    }
                    None => j.status_detail = "Completed".to_string(),
                }
            })
            .await;
        }
        Some(Ok(status)) => {
            remove_partial().await;
            fail_job(
                app,
                &job.id,
                "FFmpeg failed",
                format!("FFmpeg exited with code {:?}.\n{}", status.code(), stderr_log),
            )
            .await;
        }
        Some(Err(e)) => {
            fail_job(app, &job.id, "Process error", format!("Error waiting for FFmpeg: {}", e)).await;
        }
        None => {}
    }
}

#[tauri::command]
pub async fn run_command(
    app: AppHandle,
    command_id: String,
    files: Vec<String>,
) -> Result<Vec<JobItem>, String> {
    enqueue_and_run(app, command_id, files, false).await
}

#[tauri::command]
pub async fn cancel_job(app: AppHandle, job_id: String) -> Result<(), String> {
    let queue_state = app.state::<Arc<AppJobQueue>>().inner().clone();

    if let Some(tx) = queue_state.cancel_tokens.lock().await.get(&job_id) {
        let _ = tx.send(true);
        return Ok(());
    }

    update_job(&app, &job_id, |j| {
        if j.state == JobState::Queued {
            j.state = JobState::Cancelled;
            j.status_detail = "Cancelled before start".to_string();
        }
    })
    .await;

    Ok(())
}

#[tauri::command]
pub async fn get_jobs(app: AppHandle) -> Result<Vec<JobItem>, String> {
    let queue_state = app.state::<Arc<AppJobQueue>>();
    let jobs = queue_state.jobs.lock().await.clone();
    Ok(jobs)
}

#[tauri::command]
pub async fn clear_finished_jobs(app: AppHandle) -> Result<Vec<JobItem>, String> {
    let queue_state = app.state::<Arc<AppJobQueue>>();
    {
        let mut jobs = queue_state.jobs.lock().await;
        jobs.retain(|j| j.state == JobState::Queued || j.state == JobState::Running);
    }
    emit_jobs_state(&app).await;
    let jobs = queue_state.jobs.lock().await.clone();
    Ok(jobs)
}

#[tauri::command]
pub async fn get_missing_ffmpeg_alert(app: AppHandle) -> Result<Option<String>, String> {
    let queue_state = app.state::<Arc<AppJobQueue>>();
    let alert = queue_state.missing_ffmpeg_alert.lock().await.clone();
    Ok(alert)
}

#[tauri::command]
pub fn open_main_window_from_popup(app: AppHandle, tab: Option<String>) -> Result<(), String> {
    let queue_state = app.state::<Arc<AppJobQueue>>();
    queue_state
        .main_window_opened_manually
        .store(true, Ordering::SeqCst);
    show_window(&app, "main");
    if let Some(popup_win) = app.get_webview_window("popup") {
        let _ = popup_win.hide();
    }
    if let Some(target_tab) = tab {
        let _ = app.emit("navigate-tab", target_tab);
    }
    Ok(())
}
