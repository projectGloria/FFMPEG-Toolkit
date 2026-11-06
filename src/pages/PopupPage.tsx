import React from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AlertTriangle, Ban, CheckCircle2, ExternalLink, FolderOpen, Loader2, XCircle } from "lucide-react";
import { api, JobItem } from "../lib/tauri";
import { Button, IconButton, ProgressBar } from "../components/ui";

interface Props {
  jobs: JobItem[];
  missingFfmpegError: string | null;
}

const closePopup = () => getCurrentWindow().close().catch(console.error);
const openMain = (tab: string) => api.openMainWindowFromPopup(tab).catch(console.error);

export const PopupPage: React.FC<Props> = ({ jobs, missingFfmpegError }) => {
  // Derived each render: running > queued > most recent.
  const job = jobs.find((j) => j.state === "running") ?? jobs.find((j) => j.state === "queued") ?? jobs[jobs.length - 1];
  const done = jobs.filter((j) => j.state === "completed").length;
  const finished = job && (job.state === "completed" || job.state === "failed" || job.state === "cancelled");

  if (missingFfmpegError) {
    return (
      <Shell>
        <div className="flex items-start gap-3">
          <img src="/app-icon.png" alt="" className="h-9 w-9 shrink-0 rounded-lg" />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5 text-sm font-semibold text-rose-400">
              <AlertTriangle className="h-4 w-4" /> FFmpeg not found
            </div>
            <p className="mt-1 line-clamp-3 text-xs text-zinc-400">{missingFfmpegError}</p>
          </div>
        </div>
        <Footer>
          <Button size="sm" variant="ghost" onClick={closePopup}>
            Dismiss
          </Button>
          <Button size="sm" variant="primary" onClick={() => openMain("ffmpeg")}>
            Open FFmpeg setup
          </Button>
        </Footer>
      </Shell>
    );
  }

  return (
    <Shell>
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <img src="/app-icon.png" alt="" className="h-10 w-10 shrink-0 rounded-lg" />
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="truncate text-xs font-semibold text-purple-300">{job?.command.label ?? "FFMPEG-Toolkit"}</span>
              {jobs.length > 1 && <span className="shrink-0 font-mono text-[11px] text-zinc-500">{done}/{jobs.length}</span>}
            </div>
            <div className="truncate text-sm font-medium text-zinc-100" title={job?.input_path}>
              {job?.file_name ?? "Waiting for a job…"}
            </div>
          </div>
        </div>
        <IconButton label="Open full window" onClick={() => openMain("queue")}>
          <ExternalLink className="h-4 w-4" />
        </IconButton>
      </div>

      {job && (
        <div className="space-y-1.5">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="flex min-w-0 items-center gap-1.5 text-zinc-400">
              {job.state === "running" && <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-purple-400" />}
              {job.state === "completed" && <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-emerald-500" />}
              {job.state === "failed" && <XCircle className="h-3.5 w-3.5 shrink-0 text-rose-500" />}
              <span className="truncate">{job.status_detail}</span>
            </span>
            {job.has_progress && <span className="shrink-0 font-mono tabular-nums text-zinc-300">{job.progress.toFixed(1)}%</span>}
          </div>
          <ProgressBar
            value={job.progress}
            indeterminate={job.state === "running" && !job.has_progress}
            tone={job.state === "completed" ? "success" : job.state === "failed" ? "danger" : job.state === "cancelled" ? "muted" : "accent"}
          />
          {job.error && <div className="truncate text-[11px] text-rose-400">{job.error}</div>}
        </div>
      )}

      <Footer>
        {job && !finished && (
          <Button size="sm" variant="danger" icon={<Ban className="h-3.5 w-3.5" />} onClick={() => api.cancelJob(job.id).catch(console.error)}>
            Cancel
          </Button>
        )}
        {job?.state === "completed" && (
          <Button
            size="sm"
            variant="primary"
            icon={<FolderOpen className="h-3.5 w-3.5" />}
            onClick={() => revealItemInDir(job.output_files[0] ?? job.input_path).catch(console.error)}
          >
            Show in folder
          </Button>
        )}
        {finished && (
          <Button size="sm" variant="ghost" onClick={closePopup}>
            Close
          </Button>
        )}
      </Footer>
    </Shell>
  );
};

const Shell: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="flex h-screen w-screen select-none flex-col justify-between overflow-hidden bg-zinc-950 p-4">{children}</div>
);

const Footer: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="flex items-center justify-end gap-2 border-t border-zinc-800 pt-3">{children}</div>
);
