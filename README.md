# talk-ubun

Voice dictation **workspace** for Ubuntu (X11). Toggle a global hotkey (F7), the
tray menu, or the on-screen avatar; speak, and your words appear — transcribed in
real time by [Soniox](https://soniox.com) — in a window where you can read,
scroll, and edit them. A holographic avatar reacts to your voice, and a **Draft
board** turns the transcript into a 3D idea graph via [Claude](https://claude.com).
Built with **Tauri 2** (Rust core + web/Three.js UI).

> Status: **Phases 00–03 implemented.** Sessions + editable transcript, reactive
> avatar, and Draft board all work. Phase 04 (lip-sync, TTS, VRM character) in
> progress. Roadmap below.

## Features

- **Sessions** — each work session is one growing, editable transcript, saved as
  JSON under `~/.local/share/talk-ubun/sessions/`. Stays on the current session
  until you click **+ Phiên mới**; reopen past sessions from the sidebar.
- **Live transcript** — finalized speech becomes editable, auto-saved blocks;
  per-block copy; editable session title.
- **Avatar** — a procedural holographic assistant (Three.js) that reacts to the
  mic level and shows idle / listening / thinking states. Click it to toggle.
- **Draft board** — sends the session transcript to Claude (`claude-opus-4-8`,
  structured output) and renders the returned `{summary, nodes, edges}` as a 3D
  force-directed idea graph.

## Architecture

```
Rust core (src-tauri/src)              Web UI (src/, Vite + TS + Three.js)
  audio.rs   mic capture + peak level    index.html  layout (sidebar + main + modal)
  soniox.rs  WS stream -> events         main.ts     invoke()/listen() wiring
  session.rs sessions + JSON persistence avatar.ts   Three.js holographic avatar
  draft.rs   Claude call (reqwest)       graph.ts    3d-force-graph idea graph
  inject.rs  clipboard paste             styles.css  holo dark theme
  config.rs  env / .env config
  lib.rs     state · commands · recorder thread · global-shortcut · tray
```

- Mic audio runs on a dedicated thread (the cpal stream is `!Send`) driving a
  Soniox session on a current-thread tokio runtime; transcript tokens and a
  ~20 fps `audio:level` are emitted as Tauri events to the webview.
- Finalized text is appended to the current session (persisted) and emitted as a
  `session:segment` event.
- Draft board: `generate_draft_board` reads the current session transcript and
  calls the Claude Messages API over raw HTTP (Rust has no official SDK).

## Requirements

- Ubuntu on **X11** (`echo $XDG_SESSION_TYPE` → `x11`).
- A Soniox API key — https://soniox.com.
- An Anthropic API key for the Draft board — https://claude.com (optional; the
  rest works without it).
- Node 18+ and the Tauri system deps:
  ```bash
  sudo apt install -y build-essential pkg-config libasound2-dev \
    libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
    libayatana-appindicator3-dev libsoup-3.0-dev
  ```

## Setup

```bash
cp .env.example .env      # set SONIOX_API_KEY (+ ANTHROPIC_API_KEY for Draft board)
npm install
```

## Run

```bash
npm run tauri dev         # dev: hot-reload UI + native window
# or a bundled app:
npm run tauri build
```

Press **F7** (or the tray menu, or click the avatar) to start/stop. Each finished
utterance becomes an editable block in the current session. Click **◳ Draft
board** to visualize the session as a 3D idea graph.

## Configuration (`.env`)

| Variable            | Default | Notes                                              |
|---------------------|---------|----------------------------------------------------|
| `SONIOX_API_KEY`    | —       | Required (speech-to-text).                          |
| `ANTHROPIC_API_KEY` | —       | Optional; required only for the Draft board.        |
| `TALK_LANGS`        | `vi`    | Language hints (bias only); use `vi,en` to mix.    |
| `TALK_HOTKEY`       | `f7`    | e.g. `f7`, `ctrl+alt+space`, `super+backslash`.    |
| `TALK_PASTE_METHOD` | `paste` | For `insert_to_focus`: `paste` or `type`.          |

## Roadmap

- **00 ✓** Tauri shell, live transcript window, F7 + tray.
- **01 ✓** Sessions (New, JSON persistence, sidebar), editable auto-saved segments.
- **02 ✓** Holographic avatar reacting to the mic level (`audio:level`).
  *(VRM character deferred — currently a procedural stand-in.)*
- **03 ✓** Draft board — Claude turns the transcript into a 3D idea graph.
- **04** VRM character + lip-sync, optional TTS reply, themes, perf tuning.
  *(Avatar already pauses while the board is open.)*

See the published design plan for the full vision.

## Diagnostics

Standalone checks (no GUI) under `src-tauri/examples/`:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example test_soniox -- audio.pcm
cargo run --manifest-path src-tauri/Cargo.toml --example test_inject -- type
```
