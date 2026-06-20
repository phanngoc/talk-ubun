//! Soniox real-time speech-to-text over WebSocket.
//!
//! Protocol: connect, send one JSON config message, stream raw PCM as binary
//! frames, then send an empty text frame to flush. The server streams back
//! tokens; `is_final` tokens are committed text, the rest are a live preview.

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::io::Write;
use tokio::sync::mpsc::Receiver;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const WS_URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
const MODEL: &str = "stt-rt-v5";

#[derive(Deserialize, Default)]
struct Resp {
    #[serde(default)]
    tokens: Vec<Tok>,
    #[serde(default)]
    finished: bool,
    error_code: Option<i64>,
    error_message: Option<String>,
}

#[derive(Deserialize, Default)]
struct Tok {
    #[serde(default)]
    text: String,
    #[serde(default)]
    is_final: bool,
}

/// Soniox emits special markers like `<end>` (endpoint detection) and `<fin>`
/// as tokens. These must never be inserted as literal text.
fn is_control_token(text: &str) -> bool {
    matches!(text, "<end>" | "<fin>")
}

/// Stream audio from `audio_rx` to Soniox until the channel closes (mic stopped),
/// pasting finalized text into the focused field. With `live_paste`, each newly
/// finalized chunk is pasted as you speak; otherwise the whole transcript is
/// pasted once at the end. The full transcript is logged to history either way.
pub async fn run_session(
    api_key: String,
    langs: Vec<String>,
    sample_rate: u32,
    live_paste: bool,
    paste_method: crate::inject::Method,
    mut audio_rx: Receiver<Vec<u8>>,
) -> Result<()> {
    let (ws, _) = connect_async(WS_URL).await?;
    let (mut write, mut read) = ws.split();

    let config = json!({
        "api_key": api_key,
        "model": MODEL,
        "audio_format": "pcm_s16le",
        "sample_rate": sample_rate,
        "num_channels": 1,
        "language_hints": langs,
        "enable_endpoint_detection": true,
    });
    write.send(Message::Text(config.to_string())).await?;

    // Background worker that owns the clipboard/keyboard for this session.
    // Dropping `paste_tx` (on return) ends it and restores the clipboard.
    let (paste_tx, _paste_handle) = crate::inject::spawn_paster(paste_method);

    let mut committed = String::new();
    let mut audio_done = false;

    loop {
        tokio::select! {
            // Forward microphone PCM to Soniox. When the channel closes (mic
            // stopped), send an empty frame to flush and stop reading audio.
            maybe = audio_rx.recv(), if !audio_done => {
                match maybe {
                    Some(pcm) => write.send(Message::Binary(pcm)).await?,
                    None => {
                        write.send(Message::Text(String::new())).await?;
                        audio_done = true;
                    }
                }
            }
            // Consume transcription tokens as they arrive.
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(txt))) => {
                        let resp: Resp = serde_json::from_str(&txt).unwrap_or_default();
                        if let Some(code) = resp.error_code {
                            crate::status::error(&format!("Soniox error {code}"));
                            return Err(anyhow!(
                                "Soniox error {}: {}",
                                code,
                                resp.error_message.unwrap_or_default()
                            ));
                        }
                        let mut new_final = String::new();
                        let mut preview = String::new();
                        for t in &resp.tokens {
                            // Skip Soniox control tokens (e.g. "<end>" emitted by
                            // endpoint detection) so they aren't pasted as text.
                            if is_control_token(&t.text) {
                                continue;
                            }
                            if t.is_final {
                                new_final.push_str(&t.text);
                            } else {
                                preview.push_str(&t.text);
                            }
                        }
                        if !new_final.is_empty() {
                            committed.push_str(&new_final);
                            // Paste the newly finalized chunk live. Finals are
                            // append-only, so this never retracts text.
                            if live_paste {
                                let _ = paste_tx.send(new_final);
                            }
                        }
                        // Live line: committed text dimmed, preview bright.
                        print!("\r\x1b[2K\x1b[90m{}\x1b[0m{}", committed.trim_start(), preview);
                        let _ = std::io::stdout().flush();
                        if resp.finished {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {} // ping/pong/etc.
                    Some(Err(e)) => return Err(e.into()),
                }
            }
        }
    }

    println!();
    let text = committed.trim().to_string();

    if text.is_empty() {
        crate::status::no_speech();
        println!("(no speech recognized)");
        return Ok(()); // paste_tx drops here -> clipboard restored
    }

    // In batch mode nothing has been pasted yet; paste the whole transcript now.
    if !live_paste {
        let _ = paste_tx.send(text.clone());
    }
    // End the paste session (restores clipboard after the queue drains).
    drop(paste_tx);

    match crate::history::append(&text) {
        Ok(path) => println!("✅ {} chars · saved to {}", text.chars().count(), path.display()),
        Err(e) => println!("✅ {} chars · history save failed: {e}", text.chars().count()),
    }
    crate::status::done(&text);
    Ok(())
}
