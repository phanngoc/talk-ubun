//! App configuration: Soniox key, languages, and the global toggle hotkey.
//! Everything is read from the environment (or a local `.env`) so the binary
//! can be reconfigured without recompiling.

use anyhow::{anyhow, Result};
use global_hotkey::hotkey::{Code, Modifiers};

pub struct Config {
    pub api_key: String,
    pub langs: Vec<String>,
    pub modifiers: Modifiers,
    pub code: Code,
    pub hotkey_str: String,
    pub live_paste: bool,
    pub paste_method: crate::inject::Method,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Load a local .env if present; ignore if missing.
        let _ = dotenvy::dotenv();

        let api_key = std::env::var("SONIOX_API_KEY")
            .map_err(|_| anyhow!("SONIOX_API_KEY is not set. Export it or put it in .env"))?;

        let langs = std::env::var("TALK_LANGS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            // Default: Vietnamese-first. Soniox hints only *bias* (don't
            // restrict), so this prioritizes Vietnamese while English words are
            // still recognized. Set TALK_LANGS=vi,en for heavy code-switching.
            .unwrap_or_else(|| vec!["vi".to_string()]);

        let hotkey_str =
            std::env::var("TALK_HOTKEY").unwrap_or_else(|_| "f7".to_string());
        let (modifiers, code) = parse_hotkey(&hotkey_str)?;

        // Live (incremental) paste is on by default; set TALK_LIVE_PASTE=0 to
        // paste once when you stop instead.
        let live_paste = std::env::var("TALK_LIVE_PASTE")
            .map(|v| v != "0")
            .unwrap_or(true);

        let paste_method = crate::inject::Method::from_env();

        Ok(Self {
            api_key,
            langs,
            modifiers,
            code,
            hotkey_str,
            live_paste,
            paste_method,
        })
    }
}

/// Parse a string like "ctrl+alt+space" into modifiers + a key code.
fn parse_hotkey(s: &str) -> Result<(Modifiers, Code)> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in s.split('+') {
        let p = part.trim().to_lowercase();
        match p.as_str() {
            "" => {}
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "cmd" | "win" => mods |= Modifiers::META,
            other => code = Some(key_to_code(other)?),
        }
    }

    let code = code.ok_or_else(|| anyhow!("hotkey '{}' is missing a main key", s))?;
    Ok((mods, code))
}

fn key_to_code(k: &str) -> Result<Code> {
    Ok(match k {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "backspace" => Code::Backspace,
        "period" | "." => Code::Period,
        "comma" | "," => Code::Comma,
        "slash" | "/" => Code::Slash,
        "backslash" | "\\" => Code::Backslash,
        "semicolon" | ";" => Code::Semicolon,
        "quote" | "'" => Code::Quote,
        "minus" | "-" => Code::Minus,
        "equal" | "=" => Code::Equal,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        other => return Err(anyhow!("unsupported key '{}' in hotkey", other)),
    })
}
