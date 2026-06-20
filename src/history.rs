//! Append each completed transcript to a history log so you can review what was
//! dictated. Location: $XDG_DATA_HOME/talk-ubun/history.log
//! (falls back to ~/.local/share/talk-ubun/history.log).

use anyhow::Result;
use std::io::Write;
use std::path::PathBuf;

pub fn dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{home}/.local/share")
        });
    PathBuf::from(base).join("talk-ubun")
}

/// Append `text` with a local timestamp. Returns the log file path.
pub fn append(text: &str) -> Result<PathBuf> {
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("history.log");
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)?;
    writeln!(f, "[{ts}] {text}")?;
    Ok(file)
}
