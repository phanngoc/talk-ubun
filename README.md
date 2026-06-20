# talk-ubun

Press a global hotkey, speak, and your words are transcribed in real time by
[Soniox](https://soniox.com) and pasted into whatever text field has focus
(Slack, Chrome, editors, …). Native Rust app for **Ubuntu 22.04 on X11**.

## How it works

```
[global hotkey] --toggle--> mic on (cpal/PulseAudio)
                                |
                         mono 16-bit PCM
                                |
                                v
                    WebSocket --> Soniox (stt-rt-v5)
                                |
                         final tokens
                                |
   [hotkey again] --stop--> assemble text --> clipboard + Ctrl+V into focus
```

- **Hotkey** (`global-hotkey`, X11): toggle — press once to start, again to stop.
- **Mic** (`cpal`): default input device, native rate, downmixed to mono. The
  real sample rate is sent to Soniox, so there is no resampling.
- **STT** (`tokio-tungstenite`): streams raw `pcm_s16le` over a WebSocket to
  `wss://stt-rt.soniox.com/transcribe-websocket`; commits `is_final` tokens.
- **Insert** (`arboard` + `enigo`): a per-session worker thread owns the
  clipboard + synthetic keyboard. With **live paste** (default) each finalized
  chunk is pasted via Ctrl+V as you speak; otherwise the whole transcript is
  pasted once on stop. The user's clipboard is saved on start and restored on
  stop. Reliable for Vietnamese diacritics and long text.
- **Status**: a tray icon (`ksni` StatusNotifierItem) shows recording state
  persistently and toggles dictation on click; desktop notifications
  (`notify-rust`) flash the final transcript and errors.
- **History** (`history.log`): every completed transcript is appended with a
  timestamp to `$XDG_DATA_HOME/talk-ubun/history.log`.

## Requirements

- Ubuntu 22.04 on an **X11** session (check: `echo $XDG_SESSION_TYPE` → `x11`).
  Wayland is not supported by the global-hotkey / paste path used here.
- A Soniox API key — https://soniox.com (free trial available).
- Build deps (already standard on Ubuntu desktops):
  ```bash
  sudo apt install -y build-essential pkg-config libasound2-dev
  ```

## Setup

```bash
cp .env.example .env
# edit .env and set SONIOX_API_KEY=...
```

`.env` keys:

| Variable          | Default            | Notes                                   |
|-------------------|--------------------|-----------------------------------------|
| `SONIOX_API_KEY`  | _(required)_       | Your Soniox key.                        |
| `TALK_LANGS`      | `vi`               | Language hints (bias only). Vietnamese-first; use `vi,en` for heavy code-switching. |
| `TALK_HOTKEY`     | `f7`               | e.g. `f7`, `super+backslash`, `ctrl+shift+d`. |
| `TALK_LIVE_PASTE` | `1`                | `1` = paste each chunk live; `0` = paste once on stop. |
| `TALK_NOTIFY`     | `1`                | `1` = desktop notifications on; `0` = off. |

Transcription history: `~/.local/share/talk-ubun/history.log` (review with
`cat`/`tail`). Each line is `[YYYY-MM-DD HH:MM:SS] <transcript>`.

## Build & run

```bash
cargo build --release
./target/release/talk-ubun
```

You should see `Waiting for hotkey...`. Focus a text field (Slack, Chrome),
press **F7**, speak, press F7 again — the transcript is pasted in. The live
partial transcript is printed in the terminal while you talk.

## Notes & troubleshooting

- **Paste doesn't land in Slack/Chrome?** Try `TALK_PASTE_METHOD=type` (types
  keystrokes directly instead of Ctrl+V), or increase `TALK_PASTE_DELAY_BEFORE`.
  To see which method works, run: `cargo run --example test_inject -- type`
  and focus the field during the countdown.
- **Terminals** use Ctrl+Shift+V, not Ctrl+V, so paste won't land in a terminal
  with the default. The app targets normal apps (Slack/Chrome) which use Ctrl+V.
- **Hotkey won't register / does nothing**: the combo may already be grabbed by
  GNOME. Pick another via `TALK_HOTKEY` (avoid `super+space` — input switcher).
- **No microphone found / no audio**: check `pactl list short sources` and your
  GNOME Sound settings input device.
- **Wayland**: this build is X11-only. On Wayland the hotkey/paste APIs are
  blocked; you'd need `ydotool` (uinput) and a different hotkey mechanism.
- **Clipboard**: the previous clipboard text is saved and restored after paste.

## Run on login (optional)

Create `~/.config/autostart/talk-ubun.desktop`:

```ini
[Desktop Entry]
Type=Application
Name=talk-ubun
Exec=/full/path/to/talk-ubun/target/release/talk-ubun
X-GNOME-Autostart-enabled=true
```
