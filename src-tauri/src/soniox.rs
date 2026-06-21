//! Soniox real-time STT over WebSocket → emits Tauri events to the frontend.
//!
//! Protocol: connect, send one JSON config message, stream raw PCM as binary
//! frames, then send an empty text frame to flush. Server streams back tokens;
//! `is_final` tokens are committed text, the rest are a live preview.
//!
//! Events emitted:
//!   transcript:update { committed, preview }  — on every server message
//!   transcript:final  { text }                — once, when the session ends

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
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

/// Soniox emits markers like `<end>` (endpoint detection); never show them.
fn is_control_token(text: &str) -> bool {
    matches!(text, "<end>" | "<fin>")
}

pub async fn run_session_events(
    app: AppHandle,
    api_key: String,
    langs: Vec<String>,
    sample_rate: u32,
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

    let mut committed = String::new();
    let mut audio_done = false;

    loop {
        tokio::select! {
            maybe = audio_rx.recv(), if !audio_done => {
                match maybe {
                    Some(pcm) => write.send(Message::Binary(pcm)).await?,
                    None => {
                        write.send(Message::Text(String::new())).await?;
                        audio_done = true;
                    }
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(txt))) => {
                        let resp: Resp = serde_json::from_str(&txt).unwrap_or_default();
                        if let Some(code) = resp.error_code {
                            let m = resp.error_message.unwrap_or_default();
                            let _ = app.emit("error", format!("Soniox {code}: {m}"));
                            return Err(anyhow!("Soniox error {code}: {m}"));
                        }
                        let mut new_final = String::new();
                        let mut preview = String::new();
                        for t in &resp.tokens {
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
                        }
                        let _ = app.emit("transcript:update", json!({
                            "committed": committed.trim_start(),
                            "preview": preview,
                        }));
                        if resp.finished {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                }
            }
        }
    }

    let text = committed.trim().to_string();
    // Clear the live line in the UI.
    let _ = app.emit("transcript:final", json!({ "text": text }));
    // Persist the finalized chunk to the current session and tell the UI to add
    // it as an editable segment.
    if !text.is_empty() {
        let store = app.state::<crate::session::SessionStore>();
        if let Some(seg) = store.append_segment(&text, "user") {
            let _ = app.emit("session:segment", &seg);
        }
    }
    Ok(())
}
