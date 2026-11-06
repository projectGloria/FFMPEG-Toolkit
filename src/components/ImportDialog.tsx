import React, { useState } from "react";
import { api, errorText } from "../lib/tauri";
import { Button } from "./ui";

interface Props {
  path: string;
  hasExisting: boolean;
  onClose: () => void;
  onDone: (message: string, error?: boolean) => void;
}

export const ImportDialog: React.FC<Props> = ({ path, hasExisting, onClose, onDone }) => {
  const [busy, setBusy] = useState(false);

  const doImport = async (replace: boolean) => {
    setBusy(true);
    try {
      const r = await api.importCommandsMd(path, replace);
      const parts = [`Imported ${r.imported}`];
      if (r.skipped) parts.push(`${r.skipped} duplicate${r.skipped === 1 ? "" : "s"} skipped`);
      if (r.invalid.length) parts.push(`${r.invalid.length} invalid: ${r.invalid.join("; ")}`);
      onDone(parts.join(" · "), r.invalid.length > 0);
    } catch (e) {
      onDone(errorText(e), true);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6" onMouseDown={onClose}>
      <div
        onMouseDown={(e) => e.stopPropagation()}
        className="w-full max-w-md rounded-lg border border-zinc-700 bg-zinc-900 p-5 shadow-2xl"
      >
        <h2 className="text-sm font-semibold text-zinc-50">Import commands</h2>
        <p className="mt-1 break-all font-mono text-xs text-zinc-500">{path}</p>
        <p className="mt-3 text-[13px] text-zinc-300">
          {hasExisting
            ? "Merge adds the file's commands to yours and skips exact duplicates. Replace deletes all current commands first."
            : "Commands from this file will be added."}
        </p>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          {hasExisting && (
            <Button variant="danger" onClick={() => doImport(true)} disabled={busy}>
              Replace all
            </Button>
          )}
          <Button variant="primary" onClick={() => doImport(false)} disabled={busy}>
            {hasExisting ? "Merge" : "Import"}
          </Button>
        </div>
      </div>
    </div>
  );
};
