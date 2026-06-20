//! Desktop status notifications via D-Bus (notify-rust).
//!
//! Used for transient feedback: the final transcript ("what came out") and
//! errors. Persistent recording state is shown by the tray icon instead, since
//! GNOME auto-dismisses long-lived notifications. Disable with TALK_NOTIFY=0.
//! All calls are best-effort: if there is no session bus, they silently no-op.

use notify_rust::{Notification, Timeout};

const APP_NAME: &str = "talk-ubun";

fn enabled() -> bool {
    std::env::var("TALK_NOTIFY").map(|v| v != "0").unwrap_or(true)
}

fn flash(summary: &str, body: &str, icon: &str) {
    if !enabled() {
        return;
    }
    let _ = Notification::new()
        .appname(APP_NAME)
        .summary(summary)
        .body(body)
        .icon(icon)
        .timeout(Timeout::Milliseconds(2500))
        .show();
}

pub fn done(text: &str) {
    let snippet = if text.chars().count() > 100 {
        format!("{}…", text.chars().take(100).collect::<String>())
    } else {
        text.to_string()
    };
    flash("✅ talk-ubun", &snippet, "emblem-default");
}

pub fn no_speech() {
    flash("talk-ubun", "Không nhận được giọng nói", "dialog-information");
}

pub fn error(msg: &str) {
    flash("⚠️ talk-ubun", msg, "dialog-error");
}
