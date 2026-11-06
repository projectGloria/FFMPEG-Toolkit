import React, { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AudioLines,
  Captions,
  Download,
  Film,
  Image as ImageIcon,
  ListVideo,
  Settings as SettingsIcon,
  Terminal,
  Upload,
} from "lucide-react";
import {
  api,
  AppSettings,
  CategoryInfo,
  CustomCommand,
  errorText,
  FfmpegStatus,
  FileCategory,
  JobItem,
} from "./lib/tauri";
import { Button } from "./components/ui";
import { CommandsPage } from "./pages/CommandsPage";
import { QueuePage } from "./pages/QueuePage";
import { FfmpegPage } from "./pages/FfmpegPage";
import { SettingsPage } from "./pages/SettingsPage";
import { PopupPage } from "./pages/PopupPage";
import { ImportDialog } from "./components/ImportDialog";

type View = "queue" | "ffmpeg" | "settings" | FileCategory;
const STATIC_VIEWS: readonly string[] = ["queue", "ffmpeg", "settings", "video", "audio", "image", "subtitle"];

const CATEGORY_ICONS: Record<FileCategory, React.ReactNode> = {
  video: <Film className="h-4 w-4" />,
  audio: <AudioLines className="h-4 w-4" />,
  image: <ImageIcon className="h-4 w-4" />,
  subtitle: <Captions className="h-4 w-4" />,
};

// Decided once, synchronously, before first render — the window label never changes.
const IS_POPUP = (() => {
  if (new URLSearchParams(window.location.search).get("window") === "popup") return true;
  try {
    return getCurrentWindow().label === "popup";
  } catch {
    return false;
  }
})();

export const App: React.FC = () => {
  const [view, setView] = useState<View>("video");
  // `null` until the backend answers: no duplicated DEFAULT_SETTINGS that can
  // drift from Rust's `AppSettings::default()`.
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [jobs, setJobs] = useState<JobItem[]>([]);
  const [commands, setCommands] = useState<CustomCommand[]>([]);
  const [categories, setCategories] = useState<CategoryInfo[]>([]);
  const [ffmpeg, setFfmpeg] = useState<FfmpegStatus | null>(null);
  const [missingFfmpegError, setMissingFfmpegError] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ text: string; error?: boolean } | null>(null);
  const [pendingImport, setPendingImport] = useState<string | null>(null);

  const refreshFfmpeg = useCallback(async () => {
    try {
      setFfmpeg(await api.checkFfmpeg());
    } catch (e) {
      console.error(e);
    }
  }, []);

  useEffect(() => {
    api.getSettings().then(setSettings).catch(console.error);
    api.getJobs().then(setJobs).catch(console.error);
    api.getMissingFfmpegAlert().then(setMissingFfmpegError).catch(console.error);
    if (!IS_POPUP) {
      api.getCommands().then(setCommands).catch(console.error);
      api.getCategories().then(setCategories).catch(console.error);
      refreshFfmpeg();
    }

    // Backend is the source of truth: every event carries the FULL list and
    // replaces ours wholesale — no client-side merging to get wrong.
    const unlisteners = [
      listen<JobItem[]>("jobs-updated", (e) => setJobs(e.payload)),
      listen<CustomCommand[]>("commands-updated", (e) => setCommands(e.payload)),
      listen<string | null>("missing-ffmpeg-error", (e) => setMissingFfmpegError(e.payload)),
      listen<string>("navigate-tab", (e) => {
        if (STATIC_VIEWS.includes(e.payload)) setView(e.payload as View);
      }),
    ];
    return () => {
      // listen() resolves async; unsubscribe once it has.
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, [refreshFfmpeg]);

  // Auto-dismiss the toast. Keyed on the notice object, so a new notice resets the timer.
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(null), notice.error ? 6000 : 3000);
    return () => clearTimeout(t);
  }, [notice]);

  const handleSettingsChange = async (updated: AppSettings) => {
    setSettings(updated); // optimistic; reconciled with what Rust actually stored
    try {
      setSettings(await api.setSettings(updated));
    } catch (e) {
      setNotice({ text: `Failed to save settings: ${errorText(e)}`, error: true });
    }
  };

  const handleExport = async () => {
    const path = await save({
      title: "Export commands",
      defaultPath: "ffmpeg-toolkit-commands.md",
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!path) return;
    try {
      const n = await api.exportCommandsMd(path);
      setNotice({ text: `Exported ${n} command${n === 1 ? "" : "s"}` });
    } catch (e) {
      setNotice({ text: errorText(e), error: true });
    }
  };

  const handlePickImport = async () => {
    const path = await open({
      title: "Import commands",
      multiple: false,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (typeof path === "string") setPendingImport(path);
  };

  if (IS_POPUP) {
    return <PopupPage jobs={jobs} missingFfmpegError={missingFfmpegError} />;
  }

  // Derived, not stored: recomputed each render from the single `commands` list.
  const activeJobs = jobs.filter((j) => j.state === "running" || j.state === "queued").length;
  const countFor = (c: FileCategory) => commands.filter((cmd) => cmd.category === c).length;
  const activeCategory = categories.find((c) => c.id === view);

  return (
    <div className="flex h-full">
      <aside className="flex w-56 shrink-0 flex-col border-r border-zinc-800 bg-zinc-900/70">
        <div className="flex items-center gap-2.5 px-4 py-4">
          <img src="/app-icon.png" alt="" className="h-8 w-8 rounded-lg" />
          <div className="leading-tight">
            <div className="text-sm font-semibold text-zinc-50">FFMPEG-Toolkit</div>
            <div className="text-[11px] text-zinc-500">Explorer commands</div>
          </div>
        </div>

        <nav className="flex-1 space-y-4 overflow-y-auto px-2 pb-3">
          <NavGroup>
            <NavItem
              active={view === "queue"}
              onClick={() => setView("queue")}
              icon={<ListVideo className="h-4 w-4" />}
              label="Queue"
              badge={activeJobs > 0 ? <span className="rounded-full bg-purple-600 px-1.5 text-[10px] font-semibold text-white">{activeJobs}</span> : null}
            />
          </NavGroup>

          <NavGroup title="Commands">
            {categories.map((c) => (
              <NavItem
                key={c.id}
                active={view === c.id}
                onClick={() => setView(c.id)}
                icon={CATEGORY_ICONS[c.id]}
                label={c.label}
                badge={<span className="text-[11px] tabular-nums text-zinc-500">{countFor(c.id)}</span>}
              />
            ))}
          </NavGroup>

          <NavGroup title="App">
            <NavItem
              active={view === "ffmpeg"}
              onClick={() => setView("ffmpeg")}
              icon={<Terminal className="h-4 w-4" />}
              label="FFmpeg"
              badge={
                <span
                  title={ffmpeg?.installed ? "FFmpeg found" : "FFmpeg not found"}
                  className={`h-2 w-2 rounded-full ${ffmpeg === null ? "bg-zinc-600" : ffmpeg.installed ? "bg-emerald-500" : "bg-rose-500"}`}
                />
              }
            />
            <NavItem
              active={view === "settings"}
              onClick={() => setView("settings")}
              icon={<SettingsIcon className="h-4 w-4" />}
              label="Settings"
              badge={
                settings && !settings.context_menu_enabled ? (
                  <span title="Explorer menu is off" className="h-2 w-2 rounded-full bg-amber-500" />
                ) : null
              }
            />
          </NavGroup>
        </nav>

        <div className="grid grid-cols-2 gap-1.5 border-t border-zinc-800 p-2">
          <Button size="sm" variant="ghost" icon={<Upload className="h-3.5 w-3.5" />} onClick={handleExport} disabled={commands.length === 0}>
            Export .md
          </Button>
          <Button size="sm" variant="ghost" icon={<Download className="h-3.5 w-3.5" />} onClick={handlePickImport}>
            Import .md
          </Button>
        </div>
      </aside>

      <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden">
        {activeCategory && (
          <CommandsPage
            // key: remount per category so editor/confirm state doesn't leak between lists
            key={activeCategory.id}
            category={activeCategory}
            commands={commands.filter((c) => c.category === activeCategory.id)}
            menuEnabled={settings?.context_menu_enabled ?? false}
            onGoToSettings={() => setView("settings")}
            onQueued={() => setView("queue")}
            onNotice={(text, error) => setNotice({ text, error })}
          />
        )}
        {view === "queue" && (
          <QueuePage
            jobs={jobs}
            missingFfmpegError={missingFfmpegError}
            onGoToFfmpeg={() => setView("ffmpeg")}
          />
        )}
        {view === "ffmpeg" && settings && (
          <FfmpegPage status={ffmpeg} onRefresh={refreshFfmpeg} settings={settings} onSettingsChange={handleSettingsChange} />
        )}
        {view === "settings" && settings && (
          <SettingsPage
            settings={settings}
            onSettingsChange={handleSettingsChange}
            onMenuToggled={(enabled) => setSettings((s) => (s ? { ...s, context_menu_enabled: enabled } : s))}
            onNotice={(text, error) => setNotice({ text, error })}
          />
        )}

        {notice && (
          <div
            role="status"
            className={`pointer-events-none absolute bottom-4 left-1/2 max-w-[80%] -translate-x-1/2 rounded-md border px-3 py-2 text-[13px] shadow-lg ${
              notice.error ? "border-rose-500/40 bg-rose-950 text-rose-200" : "border-zinc-700 bg-zinc-800 text-zinc-100"
            }`}
          >
            {notice.text}
          </div>
        )}
      </main>

      {pendingImport && (
        <ImportDialog
          path={pendingImport}
          hasExisting={commands.length > 0}
          onClose={() => setPendingImport(null)}
          onDone={(text, error) => {
            setPendingImport(null);
            setNotice({ text, error });
          }}
        />
      )}
    </div>
  );
};

const NavGroup: React.FC<{ title?: string; children: React.ReactNode }> = ({ title, children }) => (
  <div>
    {title && <div className="px-2 pb-1 text-[11px] font-medium uppercase tracking-wider text-zinc-500">{title}</div>}
    <div className="space-y-0.5">{children}</div>
  </div>
);

const NavItem: React.FC<{
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  badge?: React.ReactNode;
}> = ({ active, onClick, icon, label, badge }) => (
  <button
    onClick={onClick}
    aria-current={active ? "page" : undefined}
    className={`flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-[13px] transition-colors ${
      active ? "bg-zinc-800 text-zinc-50" : "text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-100"
    }`}
  >
    <span className={active ? "text-purple-400" : ""}>{icon}</span>
    <span className="flex-1 text-left">{label}</span>
    {badge}
  </button>
);

export default App;
