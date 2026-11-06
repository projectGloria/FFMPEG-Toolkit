import React, { useEffect, useState } from "react";
import { api, AppSettings, ContextMenuStatus, errorText, LaunchMode } from "../lib/tauri";
import { PageHeader, Row, Section, Toggle } from "../components/ui";

const LAUNCH_MODES: { id: LaunchMode; label: string; description: string }[] = [
  { id: "popup", label: "Compact popup", description: "Small always-on-top window with progress and cancel." },
  { id: "main_window", label: "Full window", description: "Opens this window on the Queue." },
  { id: "background", label: "Background only", description: "No window; a notification when done, then the app exits." },
];

interface Props {
  settings: AppSettings;
  onSettingsChange: (s: AppSettings) => Promise<void>;
  onMenuToggled: (enabled: boolean) => void;
  onNotice: (text: string, error?: boolean) => void;
}

export const SettingsPage: React.FC<Props> = ({ settings, onSettingsChange, onMenuToggled, onNotice }) => {
  const [menu, setMenu] = useState<ContextMenuStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.contextMenuStatus().then(setMenu).catch(console.error);
  }, []);

  const toggleMenu = async (enabled: boolean) => {
    setBusy(true);
    try {
      const status = await api.setContextMenuEnabled(enabled);
      setMenu(status);
      onMenuToggled(status.enabled);
    } catch (e) {
      onNotice(errorText(e), true);
    } finally {
      setBusy(false);
    }
  };

  const patch = (p: Partial<AppSettings>) => onSettingsChange({ ...settings, ...p });

  return (
    <>
      <PageHeader title="Settings" />
      <div className="flex-1 space-y-5 overflow-y-auto px-6 py-5">
        <Section
          title="Explorer context menu"
          description={
            <>
              Registered per user (HKCU), no admin needed. On Windows 11 it's under <b className="text-zinc-300">Show more options</b> (Shift+F10).
            </>
          }
        >
          <Row
            label="Show FFMPEG-Toolkit ▶ on right-click"
            description={
              menu?.enabled
                ? `${menu.registered_commands} command${menu.registered_commands === 1 ? "" : "s"} registered`
                : "Your commands are saved but not shown in Explorer."
            }
          >
            <Toggle checked={menu?.enabled ?? settings.context_menu_enabled} onChange={toggleMenu} disabled={busy || !menu} label="Explorer menu" />
          </Row>
          {menu?.enabled && menu.registered_exe_path && !menu.path_matches && (
            <div className="px-4 py-2.5 text-xs text-amber-300">
              Registered for a different exe ({menu.registered_exe_path}). It's re-synced on every start, so restarting this build fixes it.
            </div>
          )}
        </Section>

        <Section title="When launched from Explorer">
          <div className="space-y-1 p-2">
            {LAUNCH_MODES.map((m) => (
              <label
                key={m.id}
                className={`flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 ${settings.launch_mode === m.id ? "bg-zinc-800" : "hover:bg-zinc-800/50"}`}
              >
                <input
                  type="radio"
                  name="launch_mode"
                  checked={settings.launch_mode === m.id}
                  onChange={() => patch({ launch_mode: m.id })}
                  className="mt-0.5 accent-purple-600"
                />
                <div>
                  <div className="text-[13px] font-medium text-zinc-200">{m.label}</div>
                  <div className="text-xs text-zinc-500">{m.description}</div>
                </div>
              </label>
            ))}
          </div>
          {settings.launch_mode === "popup" && (
            <Row label="Auto-close popup" description="Close the popup when every job succeeds.">
              <Toggle checked={settings.popup_auto_close} onChange={(v) => patch({ popup_auto_close: v })} label="Auto-close popup" />
            </Row>
          )}
        </Section>

        <Section title="General">
          <Row label="Notify when a job finishes" description="Failures always notify.">
            <Toggle checked={settings.notify_on_finish} onChange={(v) => patch({ notify_on_finish: v })} label="Notify on finish" />
          </Row>
          <Row label="Start with Windows">
            <Toggle checked={settings.start_with_windows} onChange={(v) => patch({ start_with_windows: v })} label="Start with Windows" />
          </Row>
        </Section>
      </div>
    </>
  );
};
