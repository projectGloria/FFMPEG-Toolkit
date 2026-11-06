import { invoke } from "@tauri-apps/api/core";

// Hand-written mirrors of the Rust serde structs. No codegen: change the Rust
// side and this file together. Struct fields stay snake_case (serde default);
// invoke() *argument* names are camelCase (Tauri converts them).

export type LaunchMode = "popup" | "main_window" | "background";

export interface AppSettings {
  custom_ffmpeg_path: string | null;
  launch_mode: LaunchMode;
  popup_auto_close: boolean;
  notify_on_finish: boolean;
  start_with_windows: boolean;
  /** Read-only from the UI's perspective; toggled via setContextMenuEnabled. */
  context_menu_enabled: boolean;
}

export interface FfmpegStatus {
  installed: boolean;
  version: string | null;
  raw_first_line: string | null;
  is_git_build: boolean;
  executable_path: string | null;
  ffprobe_available: boolean;
  ffprobe_path: string | null;
  source: string | null;
  winget_available: boolean;
}

export interface InstallLogEvent {
  line: string;
  is_done: boolean;
  success: boolean | null;
}

export interface UpdateCheckResult {
  installed_version: string | null;
  latest_version: string | null;
  is_git_build: boolean;
  update_available: boolean;
  status_message: string;
  error: string | null;
}

export interface ContextMenuStatus {
  enabled: boolean;
  registered_exe_path: string | null;
  current_exe_path: string;
  path_matches: boolean;
  registered_commands: number;
}

export type FileCategory = "video" | "audio" | "image" | "subtitle";

export interface CategoryInfo {
  id: FileCategory;
  label: string;
  extensions: string[];
}

export interface CustomCommand {
  /** Empty string = new command; the backend assigns a uuid on save. */
  id: string;
  category: FileCategory;
  label: string;
  args: string;
  output_name: string;
  output_ext: string;
  enabled: boolean;
  /** Explorer submenu name; "" = directly under FFMPEG-Toolkit ▶. */
  group: string;
}

export interface CommandPreview {
  command_line: string | null;
  output_path: string | null;
  warnings: string[];
  error: string | null;
}

export interface ImportResult {
  imported: number;
  skipped: number;
  invalid: string[];
}

export type JobState = "queued" | "running" | "completed" | "failed" | "cancelled";

export interface JobItem {
  id: string;
  command: CustomCommand;
  input_path: string;
  file_name: string;
  folder_path: string;
  output_files: string[];
  state: JobState;
  progress: number;
  has_progress: boolean;
  status_detail: string;
  error: string | null;
  command_line: string | null;
}

export const api = {
  checkFfmpeg: () => invoke<FfmpegStatus>("check_ffmpeg"),
  checkUpdate: () => invoke<UpdateCheckResult>("check_update"),
  installOrUpdateFfmpeg: (isUpdate: boolean) =>
    invoke<boolean>("install_or_update_ffmpeg", { isUpdate }),

  contextMenuStatus: () => invoke<ContextMenuStatus>("context_menu_status"),
  setContextMenuEnabled: (enabled: boolean) =>
    invoke<ContextMenuStatus>("set_context_menu_enabled", { enabled }),

  getCategories: () => invoke<CategoryInfo[]>("get_categories"),
  getCommands: () => invoke<CustomCommand[]>("get_commands"),
  saveCommand: (command: CustomCommand) =>
    invoke<CustomCommand[]>("save_command", { command }),
  deleteCommand: (id: string) => invoke<CustomCommand[]>("delete_command", { id }),
  moveCommand: (id: string, up: boolean) =>
    invoke<CustomCommand[]>("move_command", { id, up }),
  previewCommand: (command: CustomCommand, samplePath: string) =>
    invoke<CommandPreview>("preview_command", { command, samplePath }),
  exportCommandsMd: (path: string) => invoke<number>("export_commands_md", { path }),
  importCommandsMd: (path: string, replace: boolean) =>
    invoke<ImportResult>("import_commands_md", { path, replace }),
  /** Adds the category's built-in presets not already present (by label); returns the count added. */
  addPresets: (category: FileCategory) => invoke<number>("add_presets", { category }),

  runCommand: (commandId: string, files: string[]) =>
    invoke<JobItem[]>("run_command", { commandId, files }),
  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),
  getJobs: () => invoke<JobItem[]>("get_jobs"),
  clearFinishedJobs: () => invoke<JobItem[]>("clear_finished_jobs"),
  getMissingFfmpegAlert: () => invoke<string | null>("get_missing_ffmpeg_alert"),
  openMainWindowFromPopup: (tab?: string) =>
    invoke<void>("open_main_window_from_popup", { tab: tab ?? null }),

  getSettings: () => invoke<AppSettings>("get_settings"),
  setSettings: (settings: AppSettings) =>
    invoke<AppSettings>("set_settings", { settings }),
};

/** Tauri rejects with a plain string from `Result<_, String>`; normalize for display. */
export const errorText = (e: unknown): string =>
  typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
