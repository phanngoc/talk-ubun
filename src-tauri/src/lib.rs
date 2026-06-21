//! talk-ubun Tauri backend.
//!
//! Bridges the existing Rust core (mic capture + Soniox streaming) to a web
//! frontend. A global hotkey or the tray menu toggles recording; while
//! recording, transcript events are emitted to the webview.

mod audio;
mod config;
mod draft;
mod inject;
mod session;
mod soniox;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use config::Config;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    recording: AtomicBool,
    stop_tx: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    cfg: Config,
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn toggle_recording(app: AppHandle) {
    do_toggle(&app);
}

#[tauri::command]
fn is_recording(state: State<AppState>) -> bool {
    state.recording.load(Ordering::SeqCst)
}

/// Paste text into whatever window currently has focus (clipboard + Ctrl+V).
#[tauri::command]
fn insert_to_focus(text: String) {
    std::thread::spawn(move || {
        let (tx, handle) = inject::spawn_paster(inject::Method::from_env());
        let _ = tx.send(text);
        drop(tx);
        let _ = handle.join();
    });
}

// ---------------------------------------------------------------- sessions

#[tauri::command]
fn new_session(store: State<session::SessionStore>) -> session::SessionMeta {
    store.new_session()
}

#[tauri::command]
fn list_sessions(store: State<session::SessionStore>) -> Vec<session::SessionMeta> {
    store.list()
}

#[tauri::command]
fn current_session(store: State<session::SessionStore>) -> session::Session {
    store.current()
}

#[tauri::command]
fn switch_session(
    store: State<session::SessionStore>,
    id: String,
) -> Result<session::Session, String> {
    store.switch(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_segment(
    store: State<session::SessionStore>,
    session_id: String,
    seg_id: String,
    text: String,
) -> Result<(), String> {
    store
        .update_segment(&session_id, &seg_id, &text)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_session_title(
    store: State<session::SessionStore>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    store
        .set_title(&session_id, &title)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- draft board

#[tauri::command]
async fn generate_draft_board(app: AppHandle) -> Result<draft::DraftBoard, String> {
    // Read transcript + key without holding any State guard across the await.
    let transcript = {
        let store = app.state::<session::SessionStore>();
        store
            .current()
            .segments
            .iter()
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };
    if transcript.trim().is_empty() {
        return Err("Phiên chưa có nội dung để vẽ.".to_string());
    }
    let key = {
        let st = app.state::<AppState>();
        st.cfg.anthropic_key.clone()
    }
    .ok_or_else(|| "ANTHROPIC_API_KEY chưa cấu hình trong .env".to_string())?;

    draft::generate(&key, &transcript)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- toggle / record

fn do_toggle(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.recording.load(Ordering::SeqCst) {
        if let Some(tx) = state.stop_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        state.recording.store(false, Ordering::SeqCst);
        emit_state(app, "idle");
        set_tray_recording(app, false);
    } else {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        *state.stop_tx.lock().unwrap() = Some(stop_tx);
        state.recording.store(true, Ordering::SeqCst);
        emit_state(app, "listening");
        set_tray_recording(app, true);

        let app2 = app.clone();
        let api = state.cfg.api_key.clone();
        let langs = state.cfg.langs.clone();
        std::thread::spawn(move || recorder_run(app2, api, langs, stop_rx));
    }
}

fn emit_state(app: &AppHandle, s: &str) {
    let _ = app.emit("state:changed", s);
}

/// Runs on its own thread: owns the (!Send) cpal stream and drives the Soniox
/// session on a current-thread tokio runtime. Stops when `stop_rx` fires.
fn recorder_run(
    app: AppHandle,
    api: String,
    langs: Vec<String>,
    stop_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = app.emit("error", format!("runtime: {e}"));
            return;
        }
    };

    let app_rec = app.clone();
    rt.block_on(async move {
        let (audio_tx, audio_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(512);
        let level = Arc::new(AtomicU32::new(0));
        let (stream, sample_rate) = match audio::build_input_stream(audio_tx, level.clone()) {
            Ok(x) => x,
            Err(e) => {
                let _ = app_rec.emit("error", format!("mic: {e}"));
                return;
            }
        };
        use cpal::traits::StreamTrait;
        if let Err(e) = stream.play() {
            let _ = app_rec.emit("error", format!("mic play: {e}"));
            return;
        }

        // Emit the mic peak level ~20fps so the avatar can react to the voice.
        let app_lvl = app_rec.clone();
        let level_for_task = level.clone();
        let emitter = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(50));
            loop {
                tick.tick().await;
                let v = f32::from_bits(level_for_task.load(Ordering::Relaxed));
                let _ = app_lvl.emit("audio:level", v);
            }
        });

        let session =
            soniox::run_session_events(app_rec.clone(), api, langs, sample_rate, audio_rx);
        tokio::pin!(session);

        tokio::select! {
            _ = stop_rx => {
                drop(stream); // closes the audio channel -> session flushes
                let _ = (&mut session).await;
            }
            r = &mut session => {
                if let Err(e) = r {
                    let _ = app_rec.emit("error", format!("{e:#}"));
                }
                drop(stream);
            }
        }

        emitter.abort();
        let _ = app_rec.emit("audio:level", 0.0f32);
    });

    // Ensure we end in the idle state even if the session ended on its own.
    if let Some(st) = app.try_state::<AppState>() {
        st.recording.store(false, Ordering::SeqCst);
    }
    emit_state(&app, "idle");
    set_tray_recording(&app, false);
}

// ---------------------------------------------------------------- tray

fn set_tray_recording(app: &AppHandle, recording: bool) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(if recording {
            "talk-ubun — đang ghi"
        } else {
            "talk-ubun — sẵn sàng"
        }));
    }
}

// ---------------------------------------------------------------- entry

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            std::process::exit(1);
        }
    };
    let hotkey = parse_shortcut(&cfg.hotkey_str).unwrap_or_else(|| Shortcut::new(None, Code::F7));
    let hk_handler = hotkey.clone();
    let hk_setup = hotkey;

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed && *shortcut == hk_handler {
                        do_toggle(app);
                    }
                })
                .build(),
        )
        .manage(AppState {
            recording: AtomicBool::new(false),
            stop_tx: Mutex::new(None),
            cfg,
        })
        .manage(session::SessionStore::load_or_create())
        .invoke_handler(tauri::generate_handler![
            toggle_recording,
            is_recording,
            insert_to_focus,
            new_session,
            list_sessions,
            current_session,
            switch_session,
            update_segment,
            set_session_title,
            generate_draft_board
        ])
        .setup(move |app| {
            // Global hotkey (registering from Rust needs no capability entry).
            if let Err(e) = app.global_shortcut().register(hk_setup) {
                eprintln!("hotkey register failed: {e}");
            }

            // Tray icon + menu.
            let toggle_i = MenuItem::with_id(app, "toggle", "🎙  Bật / Tắt ghi", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_i, &quit_i])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("talk-ubun — sẵn sàng")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "toggle" => do_toggle(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ---------------------------------------------------------------- hotkey parse

fn parse_shortcut(s: &str) -> Option<Shortcut> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;
    for part in s.split('+') {
        match part.trim().to_lowercase().as_str() {
            "" => {}
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "cmd" | "win" => mods |= Modifiers::META,
            k => code = Some(key_code(k)?),
        }
    }
    Some(Shortcut::new(
        if mods.is_empty() { None } else { Some(mods) },
        code?,
    ))
}

fn key_code(k: &str) -> Option<Code> {
    use Code::*;
    Some(match k {
        "space" => Space,
        "enter" | "return" => Enter,
        "tab" => Tab,
        "f1" => F1, "f2" => F2, "f3" => F3, "f4" => F4, "f5" => F5, "f6" => F6,
        "f7" => F7, "f8" => F8, "f9" => F9, "f10" => F10, "f11" => F11, "f12" => F12,
        "a" => KeyA, "b" => KeyB, "c" => KeyC, "d" => KeyD, "e" => KeyE, "f" => KeyF,
        "g" => KeyG, "h" => KeyH, "i" => KeyI, "j" => KeyJ, "k" => KeyK, "l" => KeyL,
        "m" => KeyM, "n" => KeyN, "o" => KeyO, "p" => KeyP, "q" => KeyQ, "r" => KeyR,
        "s" => KeyS, "t" => KeyT, "u" => KeyU, "v" => KeyV, "w" => KeyW, "x" => KeyX,
        "y" => KeyY, "z" => KeyZ,
        "0" => Digit0, "1" => Digit1, "2" => Digit2, "3" => Digit3, "4" => Digit4,
        "5" => Digit5, "6" => Digit6, "7" => Digit7, "8" => Digit8, "9" => Digit9,
        _ => return None,
    })
}
