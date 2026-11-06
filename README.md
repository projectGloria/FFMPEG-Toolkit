# FFMPEG-Toolkit

**FFMPEG-Toolkit** is a Windows 10/11 desktop utility built with **Tauri v2** (Rust backend; TypeScript + React + Vite + Tailwind frontend). You define your own ffmpeg commands per file type, and they show up in the Windows Explorer right-click menu.

---

## Features

1. **Your own right-click commands, grouped by file type**
   - There are four categories: **Video**, **Audio**, **Image**, and **Subtitle**. Each one registers for its own extensions plus the Windows perceived type (`video`/`audio`/`image`).
   - A command has a **menu text**, an **output file** (name template + extension), and **ffmpeg arguments** with placeholders:

     | Placeholder | Meaning |
     |---|---|
     | `{input}`  | Full path of the right-clicked file |
     | `{output}` | Output path: `<folder>\<output name>.<ext>`, gets ` (1)`, ` (2)`… instead of overwriting |
     | `{name}`   | Input file name without extension |
     | `{dir}`    | Folder of the input file |
     | `{ext}`    | Input extension without the dot |

     Example, *Convert to WhatsApp video* (output `{name}_whatsapp.mp4`):
     ```
     -i {input} -c:v libx264 -profile:v baseline -level 3.0 -pix_fmt yuv420p -vf "scale=-2:720" -c:a aac -b:a 128k -movflags +faststart {output}
     ```
   - Arguments are split like a command line (`"..."` groups, backslashes are literal) **before** placeholders are substituted. A path with spaces therefore stays a single argument, and you never need to quote `{input}`/`{output}`.
   - The editor shows a live preview of the exact resolved command. Use **▶ Run on files…** to test a command without going through Explorer.
   - You can enable/disable, reorder, duplicate, and delete commands. The registry is re-synced on every change.

2. **Export / import as Markdown**
   - **Export .md** writes every command, grouped by category, to one human-readable file.
   - **Import .md** reads it back, either **Merge** (exact duplicates are skipped) or **Replace all**. Lines the importer doesn't recognise are ignored, so you can add notes to the file.

3. **Job queue**
   - Selecting several files in Explorer funnels them all into one sequential queue (`tauri-plugin-single-instance`).
   - Live progress comes from `-progress pipe:1` against the ffprobe duration. Inputs without a duration (still images) show an indeterminate bar.
   - Jobs can be cancelled, which kills ffmpeg and deletes the partial output. A failed job shows the resolved command line and the end of ffmpeg's stderr.

4. **Launch modes for Explorer invocations**: compact always-on-top popup (default), full window, or background-only with toast notifications.

5. **FFmpeg detection + updater**: detects `ffmpeg`/`ffprobe` from `PATH` or a custom path, checks for updates against gyan.dev, and installs or upgrades via `winget install|upgrade --id Gyan.FFmpeg -e` with a live log.

---

## Prerequisites

- **OS**: Windows 10 or Windows 11 (64-bit)
- **WebView2**: Microsoft Edge WebView2 Runtime (pre-installed on Windows 10/11)
- **Node.js**: v18+ and `npm`
- **Rust**: Stable Rust toolchain with MSVC build tools (`x86_64-pc-windows-msvc`)

---

## Development & Building

```powershell
npm install
npm run tauri dev       # dev mode with hot reload
npm run tauri build     # .exe / .msi / NSIS installers
cargo test --manifest-path src-tauri/Cargo.toml   # tokenizer, placeholder, Markdown round-trip tests
```

---

## App Icon

The source is `FFMPEG-Toolkit.ico` in the project root. It contains hand-tuned frames from 16 to 256 px. Don't run `tauri icon` on it: that command expects a PNG of at least 1024 px and would replace the small sizes.

To change the icon:
1. Replace `FFMPEG-Toolkit.ico` and copy it to `src-tauri/icons/icon.ico`. This is what the exe, the installer, the window, the tray and the Explorer menu use.
2. Extract the embedded PNG frames to `src-tauri/icons/{32x32,64x64,128x128,128x128@2x,icon}.png` and to `public/app-icon.png` (used by the in-app header and popup).
3. If Explorer keeps showing the old icon, run `ie4uinit.exe -show`.

---

## Windows Registry Structure (Explorer Context Menu)

Everything is written under `HKEY_CURRENT_USER` (no elevation). For every target of every category that has at least one enabled command:

```text
HKCU\Software\Classes\SystemFileAssociations\<.ext | video | audio | image>\shell\FFMPEGToolkit
  ├─ MUIVerb     = "FFMPEG-Toolkit"
  ├─ SubCommands = ""
  ├─ Icon        = "\"C:\path\to\ffmpeg-toolkit.exe\",0"
  ├─ ExePath     = "C:\path\to\ffmpeg-toolkit.exe"
  └─ shell
       ├─ 00_<id8>   MUIVerb = "<menu text>"
       │    └─ command (Default) = "\"C:\path\to\ffmpeg-toolkit.exe\" --cmd <command-id> \"%1\""
       └─ 01_<id8>   ...
```

- **Sync model**: every change to the commands deletes the `FFMPEGToolkit` key under every target and writes it again. A sync also runs on every app start, which repairs the entries if the exe has moved. With the menu turned off in Settings, the keys are only removed.
- **Sub-entry icons**: `SHOW_SUBENTRY_ICONS` in `src-tauri/src/context_menu.rs`.
- **`.ts` is not registered**: it's TypeScript far more often than MPEG-TS.
- **Windows 11**: classic shell verbs appear under **Show more options** (`Shift + F10`).

---

## Project Structure

```text
src/
├─ App.tsx                      # Sidebar shell; holds jobs/commands/settings mirrored from Rust
├─ lib/tauri.ts                 # Typed invoke wrappers + TS mirrors of the Rust structs
├─ components/
│  ├─ ui.tsx                    # Button, Toggle, Section, Row, ProgressBar…
│  ├─ CommandEditor.tsx         # New/edit command modal with live preview
│  ├─ ImportDialog.tsx          # Merge vs replace
│  └─ JobQueueList.tsx
└─ pages/
   ├─ CommandsPage.tsx          # One per category (Video/Audio/Image/Subtitle)
   ├─ QueuePage.tsx
   ├─ FfmpegPage.tsx            # Detection, update check, WinGet log
   ├─ SettingsPage.tsx          # Explorer menu toggle, launch mode, notifications
   └─ PopupPage.tsx             # Compact progress window
src-tauri/src/
├─ main.rs                      # Setup, tray, single-instance, `--cmd` CLI routing
├─ commands.rs                  # Command model, tokenizer, placeholders, Markdown, CRUD
├─ actions.rs                   # Job queue + generic ffmpeg runner
├─ context_menu.rs              # HKCU registry sync
├─ settings.rs                  # tauri-plugin-store settings
├─ ffmpeg.rs                    # Detection, version parsing, WinGet
└─ update.rs                    # gyan.dev release check
```
