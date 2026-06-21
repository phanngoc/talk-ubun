//! Insert text into the focused window.
//!
//! Two methods (set `TALK_PASTE_METHOD`):
//!   - "paste" (default): put text on the clipboard and simulate Ctrl+V. Fast,
//!     handles long text and Vietnamese diacritics well. Some Electron apps can
//!     be picky about synthetic Ctrl+V timing.
//!   - "type": synthesize the keystrokes directly (no clipboard). More
//!     compatible with stubborn apps, slightly slower.
//!
//! A single background thread owns the clipboard + synthetic keyboard for the
//! whole dictation session: in paste mode it saves the user's clipboard on
//! start and restores it after the session (closing the `Sender`).
//! enigo/arboard are blocking and tied to their owning thread, so they live here.

use anyhow::{anyhow, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Paste,
    Type,
}

impl Method {
    pub fn from_env() -> Self {
        match std::env::var("TALK_PASTE_METHOD").as_deref() {
            Ok("type") => Method::Type,
            _ => Method::Paste,
        }
    }
}

fn delay_ms(var: &str, default: u64) -> Duration {
    let ms = std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default);
    Duration::from_millis(ms)
}

/// Spawn the insert worker. Send chunks of text to insert; drop the `Sender` to
/// end the session (restores the clipboard in paste mode).
pub fn spawn_paster(method: Method) -> (Sender<String>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<String>();
    let handle = thread::spawn(move || paster_loop(rx, method));
    (tx, handle)
}

fn paster_loop(rx: Receiver<String>, method: Method) {
    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("\nkeyboard init failed, insertion disabled: {e}");
            return;
        }
    };

    match method {
        Method::Type => {
            for text in rx {
                if text.is_empty() {
                    continue;
                }
                match enigo.text(&text) {
                    Ok(_) => eprintln!("[insert] typed {} chars", text.chars().count()),
                    Err(e) => eprintln!("\ntype failed: {e}"),
                }
            }
        }
        Method::Paste => {
            let mut clipboard = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("\nclipboard init failed: {e}");
                    return;
                }
            };
            let original = clipboard.get_text().ok();
            let before = delay_ms("TALK_PASTE_DELAY_BEFORE", 150);
            let after = delay_ms("TALK_PASTE_DELAY_AFTER", 120);

            for text in rx {
                if text.is_empty() {
                    continue;
                }
                match paste_once(&mut clipboard, &mut enigo, &text, before, after) {
                    Ok(_) => eprintln!("[insert] pasted {} chars", text.chars().count()),
                    Err(e) => eprintln!("\npaste failed: {e}"),
                }
            }

            // Let the last paste settle before touching the clipboard again.
            thread::sleep(delay_ms("TALK_PASTE_SETTLE", 350));
            if std::env::var("TALK_PASTE_RESTORE").as_deref() != Ok("0") {
                if let Some(prev) = original {
                    let _ = clipboard.set_text(prev);
                    thread::sleep(Duration::from_millis(80));
                }
            }
        }
    }
}

fn paste_once(
    clipboard: &mut arboard::Clipboard,
    enigo: &mut Enigo,
    text: &str,
    before: Duration,
    after: Duration,
) -> Result<()> {
    clipboard.set_text(text.to_owned())?;
    thread::sleep(before);
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| anyhow!("ctrl press: {e}"))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| anyhow!("v click: {e}"))?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|e| anyhow!("ctrl release: {e}"))?;
    thread::sleep(after);
    Ok(())
}
