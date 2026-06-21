//! Conversational assistant: generate a spoken reply.
//!
//! 1. `reply()` asks Claude for a short, speakable answer to the transcript.
//! 2. `tts()` synthesizes that text with Soniox TTS (WebSocket) and returns the
//!    audio bytes (mp3). The frontend plays it and lip-syncs the avatar.

use anyhow::{anyhow, Result};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const CLAUDE_URL: &str = "https://api.anthropic.com/v1/messages";
const CLAUDE_MODEL: &str = "claude-opus-4-8";
const TTS_URL: &str = "wss://tts-rt.soniox.com/tts-websocket";
const TTS_MODEL: &str = "tts-rt-v1";

#[derive(Serialize, Clone)]
pub struct Reply {
    pub text: String,
    /// base64-encoded mp3
    pub audio_b64: String,
}

const SYSTEM: &str = "Bạn là trợ lý giọng nói thân thiện. Người dùng nói qua micro; \
hãy trả lời tự nhiên, NGẮN GỌN (1–3 câu), cùng ngôn ngữ với người dùng (mặc định tiếng Việt). \
QUAN TRỌNG: Trả lời/giải thích TRỰC TIẾP, KHÔNG hỏi lại. Nếu thiếu thông tin, hãy GIẢ ĐỊNH \
hợp lý rồi trả lời luôn (chỉ hỏi khi thật sự không thể tiếp tục). Khi người dùng nói về một \
hàm số hoặc đồ thị toán học, hãy giải thích ngắn gọn đặc điểm của nó — hệ thống sẽ tự vẽ đồ thị. \
Câu trả lời sẽ được ĐỌC THÀNH TIẾNG, nên tuyệt đối không dùng markdown, gạch đầu dòng, ký hiệu \
hay emoji — chỉ văn xuôi tự nhiên.";

/// `conversation` is the full dialogue as (role, text) pairs, role ∈ {user, assistant},
/// so Claude has the context of what it already said and doesn't repeat itself.
pub async fn reply(api_key: &str, conversation: &[(String, String)]) -> Result<String> {
    let messages: Vec<Value> = conversation
        .iter()
        .map(|(role, text)| {
            let role = if role == "assistant" { "assistant" } else { "user" };
            json!({ "role": role, "content": text })
        })
        .collect();

    let body = json!({
        "model": CLAUDE_MODEL,
        "max_tokens": 512,
        "thinking": { "type": "disabled" }, // short conversational reply — no deep reasoning
        "output_config": { "effort": "low" },
        "system": SYSTEM,
        "messages": messages,
    });

    let resp = reqwest::Client::new()
        .post(CLAUDE_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let val: Value = resp.json().await?;
    if !status.is_success() {
        let msg = val
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(anyhow!("Claude API {status}: {msg}"));
    }

    let mut text = String::new();
    if let Some(blocks) = val.get("content").and_then(|c| c.as_array()) {
        for b in blocks {
            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = b.get("text").and_then(|x| x.as_str()) {
                    text.push_str(t);
                }
            }
        }
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(anyhow!("Claude returned no reply text"));
    }
    Ok(text)
}

/// Synthesize `text` via Soniox TTS and return mp3 bytes.
pub async fn tts(api_key: &str, text: &str, voice: &str, language: &str) -> Result<Vec<u8>> {
    let (ws, _) = connect_async(TTS_URL).await?;
    let (mut write, mut read) = ws.split();

    let config = json!({
        "api_key": api_key,
        "model": TTS_MODEL,
        "language": language,
        "voice": voice,
        "audio_format": "mp3",
        "sample_rate": 24000,
        "bitrate": 128000,
        "stream_id": "s1",
    });
    write.send(Message::Text(config.to_string())).await?;
    write
        .send(Message::Text(
            json!({ "text": text, "text_end": true, "stream_id": "s1" }).to_string(),
        ))
        .await?;

    let mut audio = Vec::new();
    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(t) => {
                let v: Value = serde_json::from_str(&t).unwrap_or_default();
                if let Some(code) = v.get("error_code") {
                    let m = v
                        .get("error_message")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default();
                    return Err(anyhow!("Soniox TTS error {code}: {m}"));
                }
                if let Some(b64) = v.get("audio").and_then(|a| a.as_str()) {
                    if !b64.is_empty() {
                        audio.extend(base64::engine::general_purpose::STANDARD.decode(b64)?);
                    }
                }
                if v.get("terminated").and_then(|x| x.as_bool()) == Some(true) {
                    break;
                }
            }
            Message::Binary(b) => audio.extend(b),
            Message::Close(_) => break,
            _ => {}
        }
    }

    if audio.is_empty() {
        return Err(anyhow!("Soniox TTS returned no audio"));
    }
    Ok(audio)
}

pub fn encode_audio(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
