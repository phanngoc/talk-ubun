//! Draft board: turn a session transcript into a 3D idea graph via Claude.
//!
//! Rust has no official Anthropic SDK, so we call the Messages API over raw HTTP
//! (reqwest) and use structured outputs to force a {summary, nodes, edges} shape.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-opus-4-8";

#[derive(Serialize, Deserialize, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DraftBoard {
    pub summary: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

const SYSTEM: &str = "Bạn là trợ lý phân tích ý tưởng. Từ bản ghi giọng nói (tiếng Việt hoặc Anh), \
hãy trích ra một sơ đồ ý tưởng trực quan: các nút (ý chính, thực thể, hành động, quyết định) và \
quan hệ giữa chúng. Mỗi nút: id ngắn (n1, n2...), label ngắn gọn, kind (một trong: idea, feature, \
task, entity, question, decision), note tóm tắt 1 câu. Mỗi edge nối from→to (dùng id) với relation \
ngắn (vd: phụ thuộc, dẫn tới, gồm, liên quan). Tối đa khoảng 14 nút, ưu tiên ý quan trọng. \
summary là 1-2 câu tóm tắt toàn bộ. Trả về JSON đúng schema.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "summary": { "type": "string" },
            "nodes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": { "type": "string" },
                        "label": { "type": "string" },
                        "kind": { "type": "string" },
                        "note": { "type": "string" }
                    },
                    "required": ["id", "label", "kind", "note"]
                }
            },
            "edges": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "from": { "type": "string" },
                        "to": { "type": "string" },
                        "relation": { "type": "string" }
                    },
                    "required": ["from", "to", "relation"]
                }
            }
        },
        "required": ["summary", "nodes", "edges"]
    })
}

pub async fn generate(api_key: &str, transcript: &str) -> Result<DraftBoard> {
    let body = json!({
        "model": MODEL,
        "max_tokens": 4096,
        "thinking": { "type": "adaptive" },
        "output_config": { "format": { "type": "json_schema", "schema": schema() } },
        "system": SYSTEM,
        "messages": [{ "role": "user", "content": transcript }],
    });

    let resp = reqwest::Client::new()
        .post(API_URL)
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

    // Concatenate text content blocks (structured output lands in a text block).
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
    let text = text.trim();
    if text.is_empty() {
        return Err(anyhow!(
            "Claude returned no text (stop_reason: {:?})",
            val.get("stop_reason")
        ));
    }

    serde_json::from_str::<DraftBoard>(text)
        .map_err(|e| anyhow!("parsing board JSON failed: {e}"))
}
