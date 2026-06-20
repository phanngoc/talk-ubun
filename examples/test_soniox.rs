//! Offline test of the Soniox real-time path used by the app.
//!
//! Streams a raw PCM file (s16le, mono, 16000 Hz) to Soniox using the exact
//! same config message as src/soniox.rs, prints the live + final transcript,
//! and does NOT touch the clipboard/keyboard. Validates: API key, model name,
//! audio_format, and token parsing — without needing a mic or focused window.
//!
//! Usage: cargo run --example test_soniox -- path/to/audio_s16le_16k_mono.pcm

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::io::Write;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const WS_URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
const SAMPLE_RATE: u32 = 16000;
const CHUNK_BYTES: usize = 3200; // ~100 ms of 16 kHz mono 16-bit audio

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

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let api_key = std::env::var("SONIOX_API_KEY")
        .map_err(|_| anyhow!("SONIOX_API_KEY not set"))?;
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("usage: test_soniox <pcm_s16le_16k_mono_file>"))?;
    let pcm = std::fs::read(&path)?;
    // Mirror the app: use TALK_LANGS (default vi) so the test reflects real config.
    let langs: Vec<String> = std::env::var("TALK_LANGS")
        .ok()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| vec!["vi".to_string()]);
    println!(
        "Streaming {} bytes ({:.1}s), language_hints={:?}",
        pcm.len(),
        pcm.len() as f32 / 32000.0,
        langs
    );

    let (ws, _) = connect_async(WS_URL).await?;
    let (mut write, mut read) = ws.split();

    let config = json!({
        "api_key": api_key,
        "model": "stt-rt-v5",
        "audio_format": "pcm_s16le",
        "sample_rate": SAMPLE_RATE,
        "num_channels": 1,
        "language_hints": langs,
        "enable_endpoint_detection": true,
    });
    write.send(Message::Text(config.to_string())).await?;

    // Sender task: stream the file in ~real time, then flush with an empty frame.
    let sender = tokio::spawn(async move {
        for chunk in pcm.chunks(CHUNK_BYTES) {
            if write.send(Message::Binary(chunk.to_vec())).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
        let _ = write.send(Message::Text(String::new())).await;
    });

    let mut final_text = String::new();
    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(txt) => {
                let resp: Resp = serde_json::from_str(&txt).unwrap_or_default();
                if let Some(code) = resp.error_code {
                    return Err(anyhow!(
                        "Soniox error {}: {}",
                        code,
                        resp.error_message.unwrap_or_default()
                    ));
                }
                let mut preview = String::new();
                for t in &resp.tokens {
                    if matches!(t.text.as_str(), "<end>" | "<fin>") {
                        continue; // endpoint/control marker, not real text
                    }
                    if t.is_final {
                        final_text.push_str(&t.text);
                    } else {
                        preview.push_str(&t.text);
                    }
                }
                print!("\r\x1b[2K\x1b[90m{}\x1b[0m{}", final_text.trim_start(), preview);
                let _ = std::io::stdout().flush();
                if resp.finished {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    let _ = sender.await;

    println!("\n----------------------------------------");
    println!("FINAL TRANSCRIPT: {}", final_text.trim());
    println!("----------------------------------------");
    Ok(())
}
