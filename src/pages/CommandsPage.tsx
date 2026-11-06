import React, { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowDown, ArrowUp, Copy, Pencil, Play, Plus, Sparkles, Trash2 } from "lucide-react";
import { api, CategoryInfo, CustomCommand, errorText, FileCategory } from "../lib/tauri";
import { Button, IconButton, PageHeader, Toggle } from "../components/ui";
import { CommandEditor } from "../components/CommandEditor";

// Default extension for a blank command; full presets live in Rust (presets.rs).
const DEFAULT_EXT: Record<FileCategory, string> = { video: "mp4", audio: "mp3", image: "png", subtitle: "srt" };

const blank = (category: FileCategory): CustomCommand => ({
  id: "",
  category,
  label: "",
  args: "-i {input}  {output}",
  output_name: "{name}",
  output_ext: DEFAULT_EXT[category],
  enabled: true,
  group: "",
});

interface Props {
  category: CategoryInfo;
  commands: CustomCommand[]; // already filtered to this category, in menu order
  menuEnabled: boolean;
  onGoToSettings: () => void;
  onQueued: () => void;
  onNotice: (text: string, error?: boolean) => void;
}

export const CommandsPage: React.FC<Props> = ({ category, commands, menuEnabled, onGoToSettings, onQueued, onNotice }) => {
  const [editing, setEditing] = useState<CustomCommand | null>(null);
  // Two-step delete instead of a modal: first click arms, second click deletes.
  const [armedDelete, setArmedDelete] = useState<string | null>(null);

  // Every mutation returns/broadcasts the new list; App's listener updates props.
  const run = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
    } catch (e) {
      onNotice(errorText(e), true);
    }
  };

  const handleRunOnFiles = async (cmd: CustomCommand) => {
    const picked = await open({
      title: `${cmd.label} — choose files`,
      multiple: true,
      filters: [
        { name: `${category.label} files`, extensions: category.extensions },
        { name: "All files", extensions: ["*"] },
      ],
    });
    const files = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (files.length === 0) return;
    await run(async () => {
      await api.runCommand(cmd.id, files);
      onQueued();
    });
  };

  // Merge, never replace: presets matched by label are skipped, so the user's
  // edited copy of a preset survives. The list itself arrives via `commands-updated`.
  const handleAddPresets = () =>
    run(async () => {
      const added = await api.addPresets(category.id);
      onNotice(added ? `Added ${added} preset${added === 1 ? "" : "s"}` : "All presets are already in the list");
    });

  return (
    <>
      <PageHeader
        title={`${category.label} commands`}
        subtitle={
          <>
            Shown when you right-click{" "}
            <span className="font-mono text-zinc-300">
              {category.extensions.slice(0, 6).map((e) => `.${e}`).join(" ")}
              {category.extensions.length > 6 && " …"}
            </span>
          </>
        }
        actions={
          <div className="flex gap-2">
            <Button icon={<Sparkles className="h-4 w-4" />} onClick={handleAddPresets}>
              Add presets
            </Button>
            <Button variant="primary" icon={<Plus className="h-4 w-4" />} onClick={() => setEditing(blank(category.id))}>
              New command
            </Button>
          </div>
        }
      />

      <div className="flex-1 overflow-y-auto px-6 py-5">
        {!menuEnabled && commands.length > 0 && (
          <div className="mb-4 flex items-center justify-between gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[13px] text-amber-200">
            The Explorer menu is turned off, so these commands won't appear on right-click yet.
            <Button size="sm" variant="secondary" onClick={onGoToSettings}>
              Turn on
            </Button>
          </div>
        )}

        {commands.length === 0 ? (
          <div className="mx-auto mt-12 max-w-md rounded-lg border border-dashed border-zinc-700 p-8 text-center">
            <p className="text-sm font-medium text-zinc-200">No {category.label.toLowerCase()} commands yet</p>
            <p className="mt-1 text-[13px] text-zinc-500">
              Each command becomes an entry under <span className="text-zinc-300">FFMPEG-Toolkit ▶</span> when you right-click a{" "}
              {category.label.toLowerCase()} file.
            </p>
            <div className="mt-5 flex justify-center gap-2">
              <Button variant="primary" icon={<Plus className="h-4 w-4" />} onClick={() => setEditing(blank(category.id))}>
                New command
              </Button>
              <Button variant="secondary" icon={<Sparkles className="h-4 w-4" />} onClick={handleAddPresets}>
                Add presets
              </Button>
            </div>
          </div>
        ) : (
          <ul className="divide-y divide-zinc-800 overflow-hidden rounded-lg border border-zinc-800 bg-zinc-900/60">
            {commands.map((cmd, i) => (
              <li key={cmd.id} className="group flex items-center gap-3 px-3 py-2.5">
                <Toggle
                  checked={cmd.enabled}
                  label={cmd.enabled ? "Shown in Explorer menu" : "Hidden from Explorer menu"}
                  onChange={(enabled) => run(() => api.saveCommand({ ...cmd, enabled }))}
                />
                <button className="min-w-0 flex-1 text-left" onClick={() => setEditing(cmd)}>
                  <div className={`truncate text-[13px] font-medium ${cmd.enabled ? "text-zinc-100" : "text-zinc-500"}`}>
                    {cmd.label}
                  </div>
                  <div className="truncate font-mono text-[11px] text-zinc-500">
                    {cmd.group && <span className="text-purple-300">{cmd.group} ▶ </span>}→ {cmd.output_name}.{cmd.output_ext} · {cmd.args.replace(/\s+/g, " ")}
                  </div>
                </button>
                <div className="flex shrink-0 items-center gap-0.5 opacity-60 transition-opacity group-hover:opacity-100">
                  <IconButton label="Run on files…" onClick={() => handleRunOnFiles(cmd)}>
                    <Play className="h-3.5 w-3.5" />
                  </IconButton>
                  <IconButton label="Move up" disabled={i === 0} onClick={() => run(() => api.moveCommand(cmd.id, true))}>
                    <ArrowUp className="h-3.5 w-3.5" />
                  </IconButton>
                  <IconButton
                    label="Move down"
                    disabled={i === commands.length - 1}
                    onClick={() => run(() => api.moveCommand(cmd.id, false))}
                  >
                    <ArrowDown className="h-3.5 w-3.5" />
                  </IconButton>
                  <IconButton label="Edit" onClick={() => setEditing(cmd)}>
                    <Pencil className="h-3.5 w-3.5" />
                  </IconButton>
                  <IconButton label="Duplicate" onClick={() => setEditing({ ...cmd, id: "", label: `${cmd.label} (copy)` })}>
                    <Copy className="h-3.5 w-3.5" />
                  </IconButton>
                  {armedDelete === cmd.id ? (
                    <Button
                      size="sm"
                      variant="danger"
                      onBlur={() => setArmedDelete(null)}
                      autoFocus
                      onClick={() => run(() => api.deleteCommand(cmd.id)).then(() => setArmedDelete(null))}
                    >
                      Delete?
                    </Button>
                  ) : (
                    <IconButton label="Delete" danger onClick={() => setArmedDelete(cmd.id)}>
                      <Trash2 className="h-3.5 w-3.5" />
                    </IconButton>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      {editing && (
        <CommandEditor
          initial={editing}
          category={category}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            onNotice("Command saved");
          }}
        />
      )}
    </>
  );
};
