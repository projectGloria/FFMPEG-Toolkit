import React, { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Ban, CheckCircle2, ChevronRight, CircleDashed, FolderOpen, Loader2, XCircle } from "lucide-react";
import { api, JobItem, JobState } from "../lib/tauri";
import { IconButton, ProgressBar } from "./ui";

const STATE_ICON: Record<JobState, React.ReactNode> = {
  queued: <CircleDashed className="h-4 w-4 text-zinc-500" />,
  running: <Loader2 className="h-4 w-4 animate-spin text-purple-400" />,
  completed: <CheckCircle2 className="h-4 w-4 text-emerald-500" />,
  failed: <XCircle className="h-4 w-4 text-rose-500" />,
  cancelled: <Ban className="h-4 w-4 text-zinc-500" />,
};

// `revealItemInDir` is covered by `opener:default`; `openPath` is NOT, so we
// reveal the output (or the input) instead of opening the bare folder.
const reveal = (job: JobItem) =>
  revealItemInDir(job.output_files[0] ?? job.input_path).catch(console.error);

export const JobQueueList: React.FC<{ jobs: JobItem[] }> = ({ jobs }) => (
  <ul className="divide-y divide-zinc-800 overflow-hidden rounded-lg border border-zinc-800 bg-zinc-900/60">
    {/* Newest first: the job you just started is the one you care about. */}
    {[...jobs].reverse().map((job) => (
      <JobRow key={job.id} job={job} />
    ))}
  </ul>
);

const JobRow: React.FC<{ job: JobItem }> = ({ job }) => {
  const [expanded, setExpanded] = useState(false);
  const active = job.state === "running" || job.state === "queued";
  const hasDetails = Boolean(job.error || job.command_line);

  return (
    <li className="px-3 py-2.5">
      <div className="flex items-center gap-3">
        {STATE_ICON[job.state]}
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="truncate text-[13px] font-medium text-zinc-100" title={job.input_path}>
              {job.file_name}
            </span>
            <span className="shrink-0 text-[11px] text-purple-300">{job.command.label}</span>
          </div>
          <div className={`truncate text-xs ${job.state === "failed" ? "text-rose-400" : "text-zinc-500"}`}>
            {job.status_detail}
          </div>
        </div>
        {job.state === "running" && job.has_progress && (
          <span className="w-12 shrink-0 text-right font-mono text-xs tabular-nums text-zinc-300">
            {job.progress.toFixed(1)}%
          </span>
        )}
        <div className="flex shrink-0 items-center gap-0.5">
          {active && (
            <IconButton label="Cancel" danger onClick={() => api.cancelJob(job.id).catch(console.error)}>
              <Ban className="h-3.5 w-3.5" />
            </IconButton>
          )}
          {job.state === "completed" && (
            <IconButton label="Show in folder" onClick={() => reveal(job)}>
              <FolderOpen className="h-3.5 w-3.5" />
            </IconButton>
          )}
          {hasDetails && (
            <IconButton label={expanded ? "Hide details" : "Show details"} onClick={() => setExpanded((v) => !v)}>
              <ChevronRight className={`h-3.5 w-3.5 transition-transform ${expanded ? "rotate-90" : ""}`} />
            </IconButton>
          )}
        </div>
      </div>

      {job.state === "running" && (
        <div className="mt-2 pl-7">
          <ProgressBar value={job.progress} indeterminate={!job.has_progress} />
        </div>
      )}

      {expanded && (
        <div className="mt-2 space-y-2 pl-7">
          {job.command_line && (
            <code className="block whitespace-pre-wrap break-all rounded bg-zinc-950 p-2 text-[11px] text-zinc-400">
              {job.command_line}
            </code>
          )}
          {job.error && (
            <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-rose-950/40 p-2 text-[11px] text-rose-300">
              {job.error}
            </pre>
          )}
        </div>
      )}
    </li>
  );
};
