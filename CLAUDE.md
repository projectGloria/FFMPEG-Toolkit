# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Windows-only Tauri v2 desktop app: Rust backend in `src-tauri/`, React 18 + TS + Vite + Tailwind frontend in `src/`. Users define their own ffmpeg commands per file category (Video / Audio / Image / Subtitle). The app registers them as a cascading Explorer right-click menu under `HKCU` and runs them in a sequential job queue. Built-in presets (`presets.rs`) are seeded into the store on first run (only when the `commands` key has never existed) and can be re-added per category via `add_presets` (merge by label); once installed they're ordinary user-owned commands.

## Commands

```powershell
npm install
npm run tauri dev                  # Vite on :1425 (strictPort) + Rust app with hot reload
npm run tauri build                # Production .exe / .msi / NSIS (runs `tsc && vite build` first)
npm run build                      # Frontend only: tsc (strict, noEmit) + vite build -> dist/
cargo test --manifest-path src-tauri/Cargo.toml               # all Rust unit tests
cargo test --manifest-path src-tauri/Cargo.toml markdown_     # single test / name filter
cargo check --manifest-path src-tauri/Cargo.toml
```

- The only tests are the Rust unit tests in `commands.rs` (tokenizer, placeholder resolution, Markdown round-trip) and `presets.rs` (every preset validates). `presets_run_in_ffmpeg` is `#[ignore]`d: it runs every preset through real ffmpeg against `FFTK_SAMPLES\sample.{mkv,flac,png,srt}`. There's no frontend test runner and no linter.
- `noUnusedLocals`/`noUnusedParameters` are on, so an unused import breaks `npm run build`.

To exercise the Explorer path by hand, run `<exe> --cmd <command-id> "<file>"`. A second invocation while the app is running is forwarded to the running instance by `tauri-plugin-single-instance`. **Every app start re-syncs the registry** (`sync_on_startup`), so running a dev build repoints an enabled menu at the dev exe.

## Architecture

### Rust is the source of truth; the frontend is a projection
- Commands live in `tauri-plugin-store` (`settings.json`, key `commands`, a `Vec<CustomCommand>` whose order is the menu order). Settings live in the same file under `app_settings`. Jobs live in memory in `AppJobQueue` (`actions.rs`).
- The frontend fetches each list once on mount. After that it **replaces** the list wholesale on events that carry the full payload: `commands-updated`, `jobs-updated`. Never update these lists optimistically on the client.
- Other events:
  - `navigate-tab`: payload is `queue | ffmpeg | settings | video | audio | image | subtitle`.
  - `missing-ffmpeg-error`
  - `queue-finished`
  - `ffmpeg-install-log`
- `src/lib/tauri.ts` holds hand-written TS mirrors of the Rust serde structs plus typed `invoke` wrappers. **Change it together with any Rust command signature or serialized struct**; there's no codegen.
  - Arg casing: Rust `command_id` ⇄ JS `commandId`; struct fields stay snake_case.
  - New commands must also be added to `generate_handler![]` in `main.rs`.

### Command model and execution (`commands.rs`, `actions.rs`)
- `CustomCommand { id, category, label, args, output_name, output_ext, enabled, group }` (`group` is `#[serde(default)]`; Markdown line `- Submenu:`). `FileCategory::registry_targets()` is the single list of extensions per category (the frontend gets it through `get_categories`; don't duplicate it in TS).
- **Tokenize first, substitute second.** `tokenize()` is a Windows-aware splitter: quotes group, backslashes are literal (POSIX shlex would mangle `C:\`). `resolve()` substitutes `{input} {output} {name} {dir} {ext}` *per token*, so paths with spaces stay a single argv entry. ffmpeg is spawned directly, never through a shell.
- `{output}` = `next_available_path(dir, output_name, output_ext)`, so outputs are never overwritten. `resolve()` runs right before a job starts, not at enqueue, so jobs in the same batch see each other's outputs.
- `preview_command` uses the same `resolve()`, so the editor preview can't drift from what actually runs.
- `JobItem.command` is a **snapshot** taken at enqueue time. Editing or deleting a command doesn't affect jobs that are already queued.
- `enqueue_and_run` is the single entry point for both the UI (`run_command`) and the CLI. It applies the launch-mode window behaviour, checks that ffmpeg is present, and spawns `process_queue_loop` only if `worker_running.swap(true)` was false, so there's exactly one worker.
  - An unknown command id (a stale registry entry) produces Failed jobs rather than silently doing nothing.
- `run_custom_command_job`:
  - It prepends `-hide_banner -nostdin -progress pipe:1 -nostats` to the user's argv.
  - Progress comes from `out_time_us` divided by the ffprobe duration. `has_progress=false` (images) means the UI shows an indeterminate bar.
  - Cancel works through a `watch` channel: kill the child, delete the partial output.
- Every spawned child process must set `creation_flags(CREATE_NO_WINDOW)` (the constant is in `ffmpeg.rs`), otherwise a console flashes in release builds.

### Registry (`context_menu.rs`)
- Full rewrite on every sync. `remove_all()` deletes every `...\SystemFileAssociations\<target>\shell\FFMPEGToolkit*` key for every target, then writes it again for each category that has ≥1 enabled command:
  - Each sub-key is `NN_<id8>`; Explorer sorts by key name, so the NN prefix carries the order.
  - **Explorer shows at most 16 static verbs per top-level cascading menu, counted across all nesting levels** (nested `SubCommands` and `ExtendedSubCommandsKey` share the budget; verified by enumerating the real shell menu via `IContextMenu`). So `CustomCommand.group` (UI: "Submenu") does *not* nest: each group becomes its own top-level root `FFMPEGToolkit_NN` labelled "FFMPEG · <group>", ungrouped commands go under "FFMPEG-Toolkit", and `menu_roots()` splits any root over 16 into "… (2)". Removal deletes every `FFMPEGToolkit*` key under each target.
  - Each command line is `"<exe>" --cmd <id> "%1"`.
- `commands.rs::commit()` triggers this after every mutation, and so does `set_context_menu_enabled`.
- `AppSettings.context_menu_enabled` is owned by `set_context_menu_enabled`. `set_settings` deliberately keeps the stored value, because the frontend's copy can be stale.
- `AppSettings` has `#[serde(default)]`: older stores (missing fields, or carrying removed ones like `crf`/`preset`) deserialize instead of resetting everything to defaults.

### Windows and lifecycle (`main.rs`)
- Two windows from one bundle, both `visible: false` at start: `main` and `popup` (`index.html?window=popup`). `App.tsx` picks `PopupPage` vs. the sidebar shell based on this. Both windows share `capabilities/default.json`.
  - `opener:default` does **not** include `allow-open-path`, so use `revealItemInDir`.
  - `dialog:default` has open/save/message but no ask/confirm. Delete uses an in-app two-step confirm.
- Close handlers are intercepted: the popup hides; the main window hides to the tray while `worker_running`. The `main_window_opened_manually` flag decides whether the app exits after a CLI-triggered queue drains (popup auto-close / background mode). Set it whenever the user deliberately surfaces the main window.

### Frontend
- Dark-only Tailwind (zinc + purple accent to match the icon). There are no `dark:` variants. Shared primitives are in `components/ui.tsx`; use them instead of long ad-hoc className strings.
- `App.tsx` holds `settings` (`null` until loaded; there are deliberately no TS-side defaults), `jobs`, `commands`, `categories`, and `ffmpeg` status. Per-category lists and counts are derived with `filter` on each render.
- `CommandsPage` is keyed by category so its local editor/confirm state resets when you switch category.

## Icon
`FFMPEG-Toolkit.ico` (repo root, PNG frames 16–256) is copied verbatim to `src-tauri/icons/icon.ico`. Its frames are byte-sliced into the `icons/*.png` files and `public/app-icon.png`. Don't run `tauri icon` on it (it wants a PNG of ≥1024 px and would replace the hand-tuned small sizes).

## Repo notes
- No `.gitignore` exists yet; `node_modules/`, `dist/`, `src-tauri/target/` are untracked artifacts and should not be committed.
- `src-tauri/gen/schemas/` is generated by the Tauri CLI.
- `FFMPEG-Toolkit.jpg` in the root is unreferenced by the build.
