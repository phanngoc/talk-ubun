//! App configuration from environment / .env: Soniox key, language hints, hotkey.
//! `.env` is loaded by walking up parent directories, so the repo-root .env is
//! found even when the app runs from `src-tauri/`.

use anyhow::{anyhow, Result};

pub struct Config {
    pub api_key: String,
    pub langs: Vec<String>,
    pub hotkey_str: String,
    pub anthropic_key: Option<String>,
    pub claude_model: String,
    pub tts_voice: String,
    pub tts_lang: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let api_key = std::env::var("SONIOX_API_KEY")
            .map_err(|_| anyhow!("SONIOX_API_KEY is not set. Put it in .env"))?;

        let langs = std::env::var("TALK_LANGS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            // Vietnamese-first. Soniox hints only bias (don't restrict), so
            // English is still recognized. Set TALK_LANGS=vi,en to add English.
            .unwrap_or_else(|| vec!["vi".to_string()]);

        let hotkey_str = std::env::var("TALK_HOTKEY").unwrap_or_else(|_| "f7".to_string());

        // Optional: only needed for the Draft board feature.
        let anthropic_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());

        // Sonnet by default for speed; set TALK_CLAUDE_MODEL=claude-opus-4-8 for max quality.
        let claude_model =
            std::env::var("TALK_CLAUDE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
        let tts_voice = std::env::var("TALK_TTS_VOICE").unwrap_or_else(|_| "Maya".to_string());
        let tts_lang = std::env::var("TALK_TTS_LANG").unwrap_or_else(|_| "vi".to_string());

        Ok(Self {
            api_key,
            langs,
            hotkey_str,
            anthropic_key,
            claude_model,
            tts_voice,
            tts_lang,
        })
    }
}
