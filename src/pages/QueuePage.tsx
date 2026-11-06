import React from "react";
import { AlertTriangle, Eraser } from "lucide-react";
import { api, JobItem } from "../lib/tauri";
import { Button, PageHeader } from "../components/ui";
import { JobQueueList } from "../components/JobQueueList";

interface Props {
  jobs: JobItem[];
  missingFfmpegError: string | null;
  onGoToFfmpeg: () => void;
}

export const QueuePage: React.FC<Props> = ({ jobs, missingFfmpegError, onGoToFfmpeg }) => {
  const finished = jobs.filter((j) => j.state !== "queued" && j.state !== "running").length;

  return (
    <>
      <PageHeader
        title="Queue"
        subtitle="Jobs from Explorer and from “Run on files…” run here one at a time."
        actions={
          <Button
            variant="ghost"
            icon={<Eraser className="h-4 w-4" />}
            disabled={finished === 0}
            onClick={() => api.clearFinishedJobs().catch(console.error)}
          >
            Clear finished
          </Button>
        }
      />
      <div className="flex-1 space-y-4 overflow-y-auto px-6 py-5">
        {missingFfmpegError && (
          <div className="flex items-center justify-between gap-3 rounded-md border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-[13px] text-rose-200">
            <span className="flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 shrink-0" />
              {missingFfmpegError}
            </span>
            <Button size="sm" onClick={onGoToFfmpeg}>
              Fix
            </Button>
          </div>
        )}
        {jobs.length === 0 ? (
          <p className="mt-12 text-center text-[13px] text-zinc-500">
            Nothing here yet. Right-click a file in Explorer, or use ▶ on a command.
          </p>
        ) : (
          <JobQueueList jobs={jobs} />
        )}
      </div>
    </>
  );
};
