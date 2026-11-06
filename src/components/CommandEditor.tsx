import React, { useEffect, useState } from "react";
import { AlertTriangle, X } from "lucide-react";
import { api, CategoryInfo, CommandPreview, CustomCommand, errorText } from "../lib/tauri";
import { Button, IconButton, inputClass } from "./ui";

const PLACEHOLDERS: [string, string][] = [
  ["{input}", "Full path of the right-clicked file"],
  ["{output}", "Output path (name + extension below, never overwrites)"],
  ["{name}", "File name without extension"],
  ["{dir}", "Folder of the input file"],
  ["{ext}", "Input extension, without the dot"],
];

interface Props {
  initial: CustomCommand;
  category: CategoryInfo;
  onClose: () => void;
  onSaved: () => void;
}

export const CommandEditor: React.FC<Props> = ({ initial, category, onClose, onSaved }) => {
  // Local draft: edits stay here until Save. The list itself is only updated by
  // the backend's `commands-updated` event, never optimistically.
  const [draft, setDraft] = useState<CustomCommand>(initial);
  const [preview, setPreview] = useState<CommandPreview | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const samplePath = `C:\\Videos\\sample clip.${category.extensions[0] ?? "bin"}`;
  const set = <K extends keyof CustomCommand>(k: K, v: CustomCommand[K]) =>
    setDraft((d) => ({ ...d, [k]: v }));

  // Debounced live preview. Rust does the resolving so the preview can't drift
  // from what actually runs. `stale` drops out-of-order responses: a slow reply
  // for an older draft must not overwrite the newer one.
  useEffect(() => {
    let stale = false;
    const t = setTimeout(() => {
      api
        .previewCommand(draft, samplePath)
        .then((p) => !stale && setPreview(p))
        .catch((e) => !stale && setPreview({ command_line: null, output_path: null, warnings: [], error: errorText(e) }));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [draft, samplePath]);

  // Esc closes. Ref-free: the handler only needs the latest onClose, and the
  // effect re-subscribes if the parent passes a new one.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setSaveError(null);
    try {
      await api.saveCommand(draft);
      onSaved();
    } catch (err) {
      setSaveError(errorText(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6" onMouseDown={onClose}>
      <form
        onSubmit={handleSave}
        // mousedown (not click) on the backdrop, so a text selection that ends outside doesn't close the modal
        onMouseDown={(e) => e.stopPropagation()}
        className="flex max-h-full w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl"
      >
        <div className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
          <h2 className="text-sm font-semibold text-zinc-50">
            {initial.id ? "Edit" : "New"} {category.label.toLowerCase()} command
          </h2>
          <IconButton type="button" label="Close" onClick={onClose}>
            <X className="h-4 w-4" />
          </IconButton>
        </div>

        <div className="space-y-4 overflow-y-auto px-4 py-4">
          <Field label="Menu text" hint="What you'll see under FFMPEG-Toolkit ▶ in Explorer.">
            <input
              autoFocus
              className={inputClass}
              value={draft.label}
              onChange={(e) => set("label", e.target.value)}
              placeholder="Convert to WhatsApp video"
            />
          </Field>

          <Field
            label="Submenu"
            hint="Optional. Commands with the same submenu get their own right-click entry, “FFMPEG · <submenu> ▶”. Explorer shows at most 16 commands per entry; extras spill into “… (2)”."
          >
            <input
              className={inputClass}
              value={draft.group}
              onChange={(e) => set("group", e.target.value)}
              placeholder="(none, shown directly under FFMPEG-Toolkit ▶)"
            />
          </Field>

          <Field label="Output file" hint="Saved next to the input. Existing files get a ' (1)' suffix instead of being overwritten.">
            <div className="flex items-center gap-1.5">
              <input
                className={`${inputClass} font-mono`}
                value={draft.output_name}
                onChange={(e) => set("output_name", e.target.value)}
                placeholder="{name}_whatsapp"
              />
              <span className="text-zinc-500">.</span>
              <input
                className={`${inputClass} w-24 font-mono`}
                value={draft.output_ext}
                onChange={(e) => set("output_ext", e.target.value)}
                placeholder="mp4"
              />
            </div>
          </Field>

          <Field label="FFmpeg arguments" hint="Everything after `ffmpeg`. Paths are substituted as single arguments, so don't quote {input}/{output}.">
            <textarea
              className={`${inputClass} h-28 resize-y font-mono text-xs leading-relaxed`}
              value={draft.args}
              onChange={(e) => set("args", e.target.value)}
              spellCheck={false}
              placeholder="-i {input} -c:v libx264 -crf 23 -c:a aac {output}"
            />
            <div className="mt-2 flex flex-wrap gap-1.5">
              {PLACEHOLDERS.map(([token, desc]) => (
                <button
                  key={token}
                  type="button"
                  title={desc}
                  onClick={() => set("args", draft.args ? `${draft.args.trimEnd()} ${token}` : token)}
                  className="rounded border border-zinc-700 bg-zinc-800 px-1.5 py-0.5 font-mono text-[11px] text-purple-300 hover:border-purple-500"
                >
                  {token}
                </button>
              ))}
            </div>
          </Field>

          <div className="rounded-md border border-zinc-800 bg-zinc-950 p-3">
            <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wider text-zinc-500">
              Preview · {samplePath}
            </div>
            {preview?.error ? (
              <p className="text-xs text-rose-400">{preview.error}</p>
            ) : (
              <>
                <code className="block whitespace-pre-wrap break-all text-xs text-zinc-200">
                  {preview?.command_line ?? "…"}
                </code>
                {preview?.warnings.map((w) => (
                  <p key={w} className="mt-2 flex items-start gap-1.5 text-xs text-amber-400">
                    <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                    {w}
                  </p>
                ))}
              </>
            )}
          </div>

          <label className="flex items-center gap-2 text-[13px] text-zinc-300">
            <input
              type="checkbox"
              checked={draft.enabled}
              onChange={(e) => set("enabled", e.target.checked)}
              className="accent-purple-600"
            />
            Show in the Explorer menu
          </label>
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-zinc-800 px-4 py-3">
          <p className="min-w-0 truncate text-xs text-rose-400">{saveError}</p>
          <div className="flex shrink-0 gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            {/* Save stays enabled even when the preview errors: the backend
                validates again and shows the definitive message. */}
            <Button type="submit" variant="primary" disabled={saving}>
              {saving ? "Saving…" : "Save command"}
            </Button>
          </div>
        </div>
      </form>
    </div>
  );
};

const Field: React.FC<{ label: string; hint?: string; children: React.ReactNode }> = ({ label, hint, children }) => (
  <div>
    <div className="mb-1 text-[13px] font-medium text-zinc-200">{label}</div>
    {children}
    {hint && <p className="mt-1 text-[11px] text-zinc-500">{hint}</p>}
  </div>
);
