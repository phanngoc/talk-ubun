//! Work sessions. Each session holds a growing, editable transcript and is
//! persisted as JSON under $XDG_DATA_HOME/talk-ubun/sessions/<id>.json.
//! The app stays on one "current" session until the user starts a new one.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

fn default_role() -> String {
    "user".to_string()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub text: String,
    #[serde(default = "default_role")]
    pub role: String, // "user" | "assistant"
    #[serde(default)]
    pub edited: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub created_at: String,
    #[serde(default)]
    pub segments: Vec<Segment>,
}

/// Lightweight session info for the sidebar.
#[derive(Clone, Serialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub segment_count: usize,
}

pub struct SessionStore {
    dir: PathBuf,
    current: Mutex<Session>,
}

fn data_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{home}/.local/share")
        });
    PathBuf::from(base).join("talk-ubun").join("sessions")
}

impl Session {
    fn new() -> Self {
        let now = chrono::Local::now();
        Session {
            id: now.format("%Y%m%d-%H%M%S%3f").to_string(),
            title: String::new(),
            created_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            segments: Vec::new(),
        }
    }

    fn display_title(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        if let Some(seg) = self.segments.first() {
            let preview: String = seg.text.chars().take(48).collect();
            if !preview.trim().is_empty() {
                return preview.trim().to_string();
            }
        }
        format!("Phiên {}", self.created_at)
    }

    fn meta(&self) -> SessionMeta {
        SessionMeta {
            id: self.id.clone(),
            title: self.display_title(),
            created_at: self.created_at.clone(),
            segment_count: self.segments.len(),
        }
    }
}

impl SessionStore {
    pub fn load_or_create() -> Self {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let current = latest_session(&dir).unwrap_or_else(Session::new);
        let store = SessionStore {
            dir,
            current: Mutex::new(current),
        };
        let _ = store.save_current();
        store
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn save(&self, s: &Session) -> Result<()> {
        std::fs::write(self.path_for(&s.id), serde_json::to_string_pretty(s)?)?;
        Ok(())
    }

    fn save_current(&self) -> Result<()> {
        let s = self.current.lock().unwrap();
        self.save(&s)
    }

    pub fn current(&self) -> Session {
        self.current.lock().unwrap().clone()
    }

    pub fn new_session(&self) -> SessionMeta {
        let s = Session::new();
        let meta = s.meta();
        let _ = self.save(&s);
        *self.current.lock().unwrap() = s;
        meta
    }

    /// Append a finalized transcript chunk to the current session. Returns the
    /// stored segment (so the frontend can render it editable).
    pub fn append_segment(&self, text: &str, role: &str) -> Option<Segment> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let mut s = self.current.lock().unwrap();
        let seg = Segment {
            id: format!("s{}", s.segments.len()),
            text: text.to_string(),
            role: role.to_string(),
            edited: false,
        };
        s.segments.push(seg.clone());
        let _ = self.save(&s);
        Some(seg)
    }

    pub fn update_segment(&self, session_id: &str, seg_id: &str, text: &str) -> Result<()> {
        {
            let mut cur = self.current.lock().unwrap();
            if cur.id == session_id {
                if let Some(seg) = cur.segments.iter_mut().find(|x| x.id == seg_id) {
                    seg.text = text.to_string();
                    seg.edited = true;
                }
                return self.save(&cur);
            }
        }
        let path = self.path_for(session_id);
        let mut s: Session = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        if let Some(seg) = s.segments.iter_mut().find(|x| x.id == seg_id) {
            seg.text = text.to_string();
            seg.edited = true;
        }
        self.save(&s)
    }

    pub fn set_title(&self, session_id: &str, title: &str) -> Result<()> {
        {
            let mut cur = self.current.lock().unwrap();
            if cur.id == session_id {
                cur.title = title.to_string();
                return self.save(&cur);
            }
        }
        let path = self.path_for(session_id);
        let mut s: Session = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        s.title = title.to_string();
        self.save(&s)
    }

    pub fn switch(&self, id: &str) -> Result<Session> {
        let s: Session = serde_json::from_str(&std::fs::read_to_string(self.path_for(id))?)?;
        *self.current.lock().unwrap() = s.clone();
        Ok(s)
    }

    pub fn list(&self) -> Vec<SessionMeta> {
        let mut metas = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("json") {
                    if let Ok(txt) = std::fs::read_to_string(&p) {
                        if let Ok(s) = serde_json::from_str::<Session>(&txt) {
                            metas.push(s.meta());
                        }
                    }
                }
            }
        }
        metas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        metas
    }
}

fn latest_session(dir: &PathBuf) -> Option<Session> {
    let mut latest: Option<Session> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Ok(txt) = std::fs::read_to_string(&p) {
                    if let Ok(s) = serde_json::from_str::<Session>(&txt) {
                        if latest.as_ref().map_or(true, |l| s.created_at > l.created_at) {
                            latest = Some(s);
                        }
                    }
                }
            }
        }
    }
    latest
}
