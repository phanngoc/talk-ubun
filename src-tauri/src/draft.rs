//! Draft board (incremental): extend an idea graph with newly-spoken content.
//!
//! Instead of re-deriving the whole board each time, we send the current board
//! (compact) plus only the new utterance and ask Claude for the DELTA — the new
//! nodes/edges to add. Saves tokens and latency, and keeps the layout stable.
//! Thinking off + low effort: this is a mechanical extraction, not deep reasoning.

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

const SYSTEM: &str = "Bạn cập nhật một sơ đồ ý tưởng theo kiểu BỔ SUNG (incremental). \
Bạn nhận: (1) board hiện tại dạng JSON (có thể rỗng) gồm các node và edge đã có; \
(2) phần nội dung người dùng VỪA nói thêm. Hãy trả về CHỈ những node MỚI và edge MỚI \
cần thêm vào — TUYỆT ĐỐI KHÔNG lặp lại node đã có trong board hiện tại. \
Node mới: id chưa trùng (vd n7, n8...), label ngắn gọn, \
kind ∈ [idea, feature, task, entity, question, decision], note 1 câu. \
Edge mới có thể nối tới id node ĐÃ CÓ trong board, hoặc giữa các node mới. \
Nếu phần mới không có ý nào đáng thêm, trả nodes=[] và edges=[]. \
summary: tóm tắt lại TOÀN BỘ board (1–2 câu). Trả về JSON đúng schema.";

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

pub async fn extend(api_key: &str, current_board_json: &str, new_text: &str) -> Result<DraftBoard> {
    let user = format!(
        "BOARD HIỆN TẠI (JSON):\n{current_board_json}\n\nNGƯỜI DÙNG VỪA NÓI:\n{new_text}"
    );
    let body = json!({
        "model": MODEL,
        "max_tokens": 1024,
        "thinking": { "type": "disabled" },
        "output_config": { "effort": "low", "format": { "type": "json_schema", "schema": schema() } },
        "system": SYSTEM,
        "messages": [{ "role": "user", "content": user }],
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
            "Claude returned no board (stop_reason: {:?})",
            val.get("stop_reason")
        ));
    }

    serde_json::from_str::<DraftBoard>(text).map_err(|e| anyhow!("parsing board JSON failed: {e}"))
}
