//! Draft board (incremental math charts): extend the set of function plots.
//!
//! The board is a math visualization. We send the current plots (compact) plus
//! the new utterance and Claude returns only NEW plots to add — 2D curves
//! (y=f(x)) or 3D surfaces (z=f(x,y)). Thinking off + low effort: mechanical.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-opus-4-8";

#[derive(Serialize, Deserialize, Clone)]
pub struct Plot {
    pub label: String,
    /// JavaScript expression. dim=2 → in terms of x; dim=3 → in terms of x and y.
    pub expr: String,
    pub dim: u8,
    pub xmin: f64,
    pub xmax: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DraftBoard {
    pub summary: String,
    #[serde(default)]
    pub plots: Vec<Plot>,
}

const SYSTEM: &str = "Bạn dựng bảng VẼ ĐỒ THỊ TOÁN HỌC theo kiểu BỔ SUNG. Nhận: (1) board hiện \
tại (JSON) gồm các đồ thị đã có; (2) nội dung người dùng vừa nói. Trả về CHỈ đồ thị MỚI cần thêm \
vào 'plots' — TUYỆT ĐỐI KHÔNG lặp lại đồ thị (expr) đã có. Mỗi plot gồm: \
label (tên ngắn, vd 'y = x²' hoặc 'paraboloid z = x²+y²'); \
dim (= 2 nếu là hàm một biến y=f(x); = 3 nếu là MẶT CONG z=f(x,y), ví dụ paraboloid khi quay \
parabol quanh trục, mặt sóng, mặt yên ngựa...); \
expr (biểu thức JavaScript: dim=2 dùng biến x — vd 'Math.sin(x)','x*x','Math.exp(-x*x)'; \
dim=3 dùng x VÀ y — vd 'x*x + y*y','Math.sin(Math.sqrt(x*x+y*y))'); \
xmin, xmax (khoảng vẽ hợp lý, dùng chung cho cả x và y khi dim=3). \
Khi người dùng nói 'dạng 3D', 'mặt cong', 'không gian 3 chiều' → dùng dim=3. \
summary: tóm tắt 1-2 câu về board. Nếu không liên quan đồ thị, plots=[]. Trả JSON đúng schema.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "summary": { "type": "string" },
            "plots": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "label": { "type": "string" },
                        "expr": { "type": "string" },
                        "dim": { "type": "integer" },
                        "xmin": { "type": "number" },
                        "xmax": { "type": "number" }
                    },
                    "required": ["label", "expr", "dim", "xmin", "xmax"]
                }
            }
        },
        "required": ["summary", "plots"]
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
        return Err(anyhow!("Claude returned no board"));
    }

    serde_json::from_str::<DraftBoard>(text).map_err(|e| anyhow!("parsing board JSON failed: {e}"))
}
