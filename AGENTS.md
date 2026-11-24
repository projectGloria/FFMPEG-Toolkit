# AGENTS.md — FFMPEG-Toolkit

Windows-only Tauri v2 desktop app (Rust backend in `src-tauri/`, React 18 + TS + Vite + Tailwind frontend in `src/`) that registers user-defined ffmpeg commands as Explorer `HKCU` right-click menus and runs them in a sequential job queue.

## Commands
- Run / Dev: `npm run tauri dev` (Vite on `:1425` strictPort + Rust hot reload)
- Test (all): `cargo test --manifest-path src-tauri/Cargo.toml`
- Test (single): `cargo test --manifest-path src-tauri/Cargo.toml markdown_` (or `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored presets_run_in_ffmpeg` with `FFTK_SAMPLES\sample.{mkv,flac,png,srt}`)
- Typecheck / Frontend build: `npm run build` (`tsc --noEmit` with `strict`/`noUnusedLocals`/`noUnusedParameters` + `vite build`; no JS test runner or linter)
- Rust check: `cargo check --manifest-path src-tauri/Cargo.toml`
- Build / Package: `npm run tauri build` (CLI test path: `<exe> --cmd <command-id> "<file>"`)

## Architecture
- `src-tauri/src/main.rs` — App entry, `generate_handler![]`, single-instance forwarding, tray/close interception, and two hidden-at-start windows (`main` and `popup` via `index.html?window=popup`).
- `src-tauri/src/commands.rs` & `presets.rs` — `CustomCommand` store (`settings.json` via `tauri-plugin-store`), Windows-aware `tokenize()` + per-token `resolve()`, `FileCategory::registry_targets()` (single source of extensions), and first-run preset seeding.
- `src-tauri/src/actions.rs`, `ffmpeg.rs`, `update.rs` — In-memory `AppJobQueue` (single worker via `worker_running`), direct ffmpeg spawn (no shell), progress/cancel via `watch`, and gyan.dev FFmpeg update check.
- `src-tauri/src/context_menu.rs` & `settings.rs` — Full-rewrite `HKCU\...\SystemFileAssociations\<ext>\shell\FFMPEGToolkit*` registry sync on startup (`sync_on_startup`) and on `commit()`; `AppSettings` with `#[serde(default)]`.
- `src/App.tsx`, `src/lib/tauri.ts`, `src/components/ui.tsx` — Frontend projection (dark-only Tailwind, no `dark:` variants). Hand-written TS mirrors in `src/lib/tauri.ts` (no codegen); keep in sync with Rust serde structs and `generate_handler![]`.
- **Version Manifests:** `gloria.toml`, `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` (keep versions synced across all listed files).

## Personal Rules (Brain)
- **Git Branching:** Never create new branches or git worktrees. Always work directly on `main` until explicitly told otherwise.
- **Versioning:** Never bump versions during normal work. The user picks the version at release time and `/monjita-github-release` sets it in every version file at once; keep version files in sync with each other otherwise.
- **Commit Format:** Write one plain sentence stating what changed and why; do not use `feat:`/`fix:`/`chore:` conventional-commit prefixes.
- **Commit & Push Policy:** Never run `git commit`, `git tag`, or `git push` unless explicitly asked; never force-push to `main`. When the user says "commit and push", verify the build and tests pass, commit on `main`, and push to `origin main`. Pushing never releases anything.
- **Releases:** Releases happen only through `/monjita-github-release`, which bumps the version, pushes, runs `gh workflow run release.yml`, and watches it until the GitHub Release is live. Never create release tags or releases by hand.
- **Path Privacy:** Always use `C:\Users\you` in documentation, comments, and examples—never hardcode the local machine's real username.
- **Release Workflow:** `.github/workflows/release.yml` must be a copy of `GloriaSetup/templates/release.yml` (shared `check`/`release` jobs from `projectGloria/GloriaSetup/.github/workflows/app-{check,release}.yml@main`, plus this repo's own `build` job). Don't hand-write release workflows or pin `gloria-setup`; see `.brain/github-actions.md`.
- **Window Chrome:** Disable native OS titlebars; build an integrated frameless titlebar with drag region, double-click maximize, and custom min/max/close controls.
- **Desktop Shell Lock:** Lock the app shell to `100vw x 100vh` (`overflow: hidden`) with internal scroll panes and `user-select: none` on UI chrome.
- **Scrollbars:** Ban raw OS scrollbars; use thin (4–6px) themed scrollbars with transparent tracks and zero layout shift.
- **Interactive Controls:** Custom-style all form controls; define explicit `hover`, `active`, `focus-visible`, and `disabled` states with snappy 120–180ms transitions.

## Playbooks (Read Before Task)
- **Packaging & Installers:** Read `.brain/building-the-project.md` before packaging, compiling release binaries, or building an installer.
- **GitHub Actions & Releases:** Read `.brain/github-actions.md` before creating/modifying `.github/workflows/*.yml`, setting up release automation, or debugging a failed release.

## Repo Guardrails & Gotchas
- **Rust Is Source of Truth:** Frontend replaces `commands`/`jobs` wholesale on `commands-updated` and `jobs-updated` events; never mutate lists optimistically or add TS-side defaults for `settings`.
- **IPC Mirroring:** Rust command args map `snake_case` -> JS `camelCase` (`command_id` -> `commandId`), while serialized struct fields stay `snake_case`; update `src/lib/tauri.ts` and `generate_handler![]` together.
- **Tokenize Before Substitute:** `tokenize()` must run before `resolve()` so `{input}`/`{output}` paths with spaces remain single argv entries; `resolve()` runs right before job start (not at enqueue) using `next_available_path`, and `JobItem.command` is snapshotted at enqueue.
- **No Console Flash:** Every spawned child process must set `creation_flags(CREATE_NO_WINDOW)` (from `ffmpeg.rs`).
- **Explorer 16-Verb Limit:** Explorer caps static verbs at 16 per top-level cascading menu across all nesting levels; `CustomCommand.group` creates separate top-level `FFMPEGToolkit_NN` roots (`menu_roots()` splits >16 into `… (2)`), never nested submenus.
- **Registry Sync Side Effects:** Every app start runs `sync_on_startup` (a dev build repoints enabled menus to the dev exe); `AppSettings.context_menu_enabled` is owned solely by `set_context_menu_enabled`, not `set_settings`.
- **Window & Plugin Capabilities:** Both windows share `capabilities/default.json`; use `revealItemInDir` (not `openPath`), use in-app two-step confirm (no `dialog` confirm), and set `main_window_opened_manually` whenever surfacing `main`.
- **Icon Pipeline:** `FFMPEG-Toolkit.ico` is copied verbatim to `src-tauri/icons/icon.ico` and byte-sliced to `icons/*.png` + `public/app-icon.png`; never run `tauri icon` on it.
- **UI Primitives & Keying:** Reuse primitives in `src/components/ui.tsx`; keep `CommandsPage` keyed by category so local editor/confirm state resets on tab switch.

## Done Criteria
Run `npm run build` and `cargo test --manifest-path src-tauri/Cargo.toml` (plus `cargo check --manifest-path src-tauri/Cargo.toml` if touching Rust), and verify `gloria.toml`, `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` report the same version.

<!-- brain-sync: 31e69a675214 -->
