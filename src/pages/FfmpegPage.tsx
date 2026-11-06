import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink, FolderSearch, RefreshCw, RotateCcw } from "lucide-react";
import { api, AppSettings, FfmpegStatus, InstallLogEvent, UpdateCheckResult } from "../lib/tauri";
import { Button, PageHeader, Row, Section } from "../components/ui";

interface Props {
  status: FfmpegStatus | null; // owned by App so the sidebar dot stays in sync
  onRefresh: () => Promise<void>;
  settings: AppSettings;
  onSettingsChange: (s: AppSettings) => Promise<void>;
}

const GYAN_URL = "https://www.gyan.dev/ffmpeg/builds/";

export const FfmpegPage: React.FC<Props> = ({ status, onRefresh, settings, onSettingsChange }) => {
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<UpdateCheckResult | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [logs, setLogs] = useState<string[]>([]);
  const logRef = useRef<HTMLDivElement | null>(null);

  // `onRefresh` is captured once; it's a stable useCallback in App.
  useEffect(() => {
    const un = listen<InstallLogEvent>("ffmpeg-install-log", (e) => {
      setLogs((prev) => [...prev, e.payload.line]);
      if (e.payload.is_done) {
        setInstalling(false);
        onRefresh();
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [onRefresh]);

  // DOM sync (scroll-to-bottom), not state derivation — a legit effect.
  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [logs]);

  const recheck = async () => {
    setChecking(true);
    await onRefresh();
    setChecking(false);
  };

  const checkUpdate = async () => {
    setCheckingUpdate(true);
    try {
      setUpdate(await api.checkUpdate());
    } catch (e) {
      setUpdate({
        installed_version: status?.version ?? null,
        latest_version: null,
        is_git_build: status?.is_git_build ?? false,
        update_available: false,
        status_message: "Failed to check for updates",
        error: String(e),
      });
    } finally {
      setCheckingUpdate(false);
    }
  };

  const pickCustomPath = async () => {
    const selected = await open({
      multiple: false,
      title: "Select ffmpeg.exe",
      filters: [{ name: "Executable", extensions: ["exe"] }],
    });
    if (typeof selected === "string") {
      await onSettingsChange({ ...settings, custom_ffmpeg_path: selected });
      await onRefresh();
    }
  };

  const clearCustomPath = async () => {
    await onSettingsChange({ ...settings, custom_ffmpeg_path: null });
    await onRefresh();
  };

  const installOrUpdate = async () => {
    setInstalling(true);
    setLogs([]);
    try {
      await api.installOrUpdateFfmpeg(Boolean(status?.installed));
    } catch (e) {
      setLogs((prev) => [...prev, `Error: ${String(e)}`]);
      setInstalling(false);
    }
  };

  const needsInstallOrUpdate = status && (!status.installed || update?.update_available);

  return (
    <>
      <PageHeader
        title="FFmpeg"
        subtitle={status?.raw_first_line ?? "Detected from your custom path, then from PATH."}
        actions={
          <Button variant="ghost" icon={<RefreshCw className={`h-4 w-4 ${checking ? "animate-spin" : ""}`} />} onClick={recheck} disabled={checking}>
            Re-check
          </Button>
        }
      />
      <div className="flex-1 space-y-5 overflow-y-auto px-6 py-5">
        <Section title="Installation">
          <Row label="Status">
            {status === null ? (
              <span className="text-[13px] text-zinc-500">Checking…</span>
            ) : status.installed ? (
              <span className="text-[13px] text-emerald-400">
                Installed{status.source === "custom" ? " · custom path" : " · PATH"}
              </span>
            ) : (
              <span className="text-[13px] text-rose-400">Not found</span>
            )}
          </Row>
          <Row label="Version">
            <span className="font-mono text-[13px] text-zinc-200">
              {status?.version ?? "—"}
              {status?.is_git_build && <span className="ml-2 font-sans text-xs text-amber-400">git build, can't compare</span>}
            </span>
          </Row>
          <Row label="ffprobe" description="Used to read duration for progress bars.">
            <span className={`text-[13px] ${status?.ffprobe_available ? "text-emerald-400" : "text-rose-400"}`}>
              {status?.ffprobe_available ? "Available" : "Not found"}
            </span>
          </Row>
          <Row label="Executable" description={<span className="font-mono">{status?.executable_path ?? "Not detected"}</span>}>
            <div className="flex gap-1.5">
              {settings.custom_ffmpeg_path && (
                <Button size="sm" variant="ghost" icon={<RotateCcw className="h-3.5 w-3.5" />} onClick={clearCustomPath}>
                  Use PATH
                </Button>
              )}
              <Button size="sm" icon={<FolderSearch className="h-3.5 w-3.5" />} onClick={pickCustomPath}>
                Choose ffmpeg.exe
              </Button>
            </div>
          </Row>
        </Section>

        <Section
          title="Updates"
          description="Compares against the latest Gyan.dev release build."
          actions={
            <>
              <Button size="sm" variant="ghost" icon={<ExternalLink className="h-3.5 w-3.5" />} onClick={() => openUrl(GYAN_URL)}>
                gyan.dev
              </Button>
              <Button size="sm" icon={<RefreshCw className={`h-3.5 w-3.5 ${checkingUpdate ? "animate-spin" : ""}`} />} onClick={checkUpdate} disabled={checkingUpdate}>
                Check for updates
              </Button>
            </>
          }
        >
          {update && (
            <div className={`px-4 py-3 text-[13px] ${update.error ? "text-rose-300" : update.update_available ? "text-amber-300" : "text-emerald-300"}`}>
              <div className="font-medium">{update.status_message}</div>
              {update.error && <div className="mt-0.5 text-xs">{update.error}</div>}
              {update.latest_version && (
                <div className="mt-0.5 font-mono text-xs text-zinc-400">
                  installed {update.installed_version ?? "none"} · latest {update.latest_version}
                </div>
              )}
            </div>
          )}
          {needsInstallOrUpdate && (
            <div className="px-4 py-3">
              {status?.winget_available ? (
                <Button variant="primary" icon={<Download className="h-4 w-4" />} onClick={installOrUpdate} disabled={installing}>
                  {installing ? "Running WinGet…" : status?.installed ? "Update with WinGet" : "Install with WinGet"}
                </Button>
              ) : (
                <Button variant="primary" icon={<ExternalLink className="h-4 w-4" />} onClick={() => openUrl(GYAN_URL)}>
                  Download from gyan.dev (WinGet unavailable)
                </Button>
              )}
            </div>
          )}
          {(installing || logs.length > 0) && (
            <div ref={logRef} className="h-48 overflow-y-auto bg-zinc-950 px-4 py-3 font-mono text-[11px] text-zinc-300">
              {logs.map((line, i) => (
                <div key={i} className="whitespace-pre-wrap break-all">
                  {line}
                </div>
              ))}
            </div>
          )}
        </Section>
      </div>
    </>
  );
};
