//! talk-ubun: dictate by toggling a global hotkey OR clicking the tray icon;
//! speech is transcribed in real time by Soniox and inserted into the focused
//! text field. A tray icon shows recording state persistently.
//!
//! State machine (toggle):
//!   Idle  --toggle--> start mic + spawn Soniox session  --> Recording
//!   Recording --toggle--> drop mic stream (closes audio channel) --> Idle
//! Dropping the cpal stream closes the PCM channel, which tells the session to
//! flush, collect the transcript, insert it, and log it to history.

mod audio;
mod config;
mod history;
mod inject;
mod soniox;
mod status;
mod tray;

use anyhow::Result;
use config::Config;
use cpal::traits::StreamTrait;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::mpsc;
use std::time::Duration;

// `stream` is an RAII guard: holding it keeps the mic open, dropping it (set to
// None) closes the audio channel and ends the session. The compiler can't see
// the drop as a use, so we allow the resulting false positives.
#[allow(unused_assignments, unused_variables)]
fn main() -> Result<()> {
    let cfg = Config::load()?;
    let rt = tokio::runtime::Runtime::new()?;

    // Register the global toggle hotkey (X11). The manager must stay alive.
    let manager = GlobalHotKeyManager::new()?;
    let hotkey = HotKey::new(Some(cfg.modifiers), cfg.code);
    manager.register(hotkey)?;
    let hotkey_id = hotkey.id();
    let hotkey_rx = GlobalHotKeyEvent::receiver();

    // Tray icon: persistent recording indicator + click-to-toggle. Toggle
    // requests (from a tray click or the menu) arrive on `toggle_rx`.
    let (toggle_tx, toggle_rx) = mpsc::channel::<()>();
    let tray = tray::start(toggle_tx);

    println!("talk-ubun ready.");
    println!("  Toggle dictation : {} (or click the tray icon)", cfg.hotkey_str);
    println!("  Languages        : {:?}", cfg.langs);
    println!(
        "  Insert           : {} · {}",
        if cfg.live_paste { "live (incremental)" } else { "batch (on stop)" },
        cfg.paste_method.label()
    );
    println!("  History          : {}", history::dir().join("history.log").display());
    println!("Waiting for hotkey / tray click...");

    let mut recording = false;
    let mut stream: Option<cpal::Stream> = None;

    loop {
        // Collapse all pending hotkey presses + tray clicks into one toggle.
        let mut toggle = false;
        while let Ok(event) = hotkey_rx.try_recv() {
            if event.id == hotkey_id && event.state == HotKeyState::Pressed {
                toggle = true;
            }
        }
        while toggle_rx.try_recv().is_ok() {
            toggle = true;
        }

        if toggle {
            if !recording {
                let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(512);
                match audio::build_input_stream(tx) {
                    Ok((s, sample_rate)) => {
                        if let Err(e) = s.play() {
                            eprintln!("cannot start microphone: {e}");
                        } else {
                            stream = Some(s);
                            recording = true;
                            tray.update(|t| t.recording = true);

                            let api = cfg.api_key.clone();
                            let langs = cfg.langs.clone();
                            let live_paste = cfg.live_paste;
                            let method = cfg.paste_method;
                            rt.spawn(async move {
                                if let Err(e) = soniox::run_session(
                                    api, langs, sample_rate, live_paste, method, rx,
                                )
                                .await
                                {
                                    eprintln!("\n⚠️  session error: {e:#}");
                                    status::error(&format!("{e}"));
                                }
                            });
                            println!("🎙️  recording... (toggle to stop)");
                        }
                    }
                    Err(e) => eprintln!("audio init failed: {e:#}"),
                }
            } else {
                // Drop the stream -> channel closes -> session flushes & inserts.
                stream = None;
                recording = false;
                tray.update(|t| t.recording = false);
                println!("⏹️  stopping; transcribing & inserting...");
            }
        }

        std::thread::sleep(Duration::from_millis(20));
    }
}
