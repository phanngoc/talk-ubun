//! Diagnose text insertion into the focused window.
//!
//!   cargo run --example test_inject            # safe: clipboard roundtrip + enigo init
//!   cargo run --example test_inject -- type    # interactive: focus Slack, watch what lands
//!
//! In interactive mode, after a 5s countdown it (1) pastes a marker via Ctrl+V
//! and (2) types a marker directly, so you can see which method works in Slack.

use std::{thread::sleep, time::Duration};

use enigo::{Direction, Enigo, Key, Keyboard, Settings};

fn main() {
    let interactive = std::env::args().nth(1).as_deref() == Some("type");

    // Clipboard roundtrip.
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            let marker = "TALKUBUN_CLIP_OK";
            match cb.set_text(marker.to_string()).and_then(|_| cb.get_text()) {
                Ok(got) => println!(
                    "clipboard: set+get {}",
                    if got == marker { "OK" } else { "MISMATCH" }
                ),
                Err(e) => println!("clipboard set/get FAILED: {e}"),
            }
        }
        Err(e) => println!("clipboard init FAILED: {e}"),
    }

    // Enigo init.
    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => {
            println!("enigo init OK");
            e
        }
        Err(e) => {
            println!("enigo init FAILED: {e}");
            return;
        }
    };

    if !interactive {
        println!("\nSafe checks done. Run with `-- type` and focus a text field to test pasting/typing.");
        return;
    }

    println!("\nFocus a Slack/Chrome text field NOW. Inserting in 5 seconds...");
    for i in (1..=5).rev() {
        println!("  {i}...");
        sleep(Duration::from_secs(1));
    }

    // Method 1: clipboard + Ctrl+V
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text("[PASTE_OK] ".to_string());
        sleep(Duration::from_millis(150));
        let _ = enigo.key(Key::Control, Direction::Press);
        let _ = enigo.key(Key::Unicode('v'), Direction::Click);
        let _ = enigo.key(Key::Control, Direction::Release);
        sleep(Duration::from_millis(400));
        println!("sent Ctrl+V (expect '[PASTE_OK] ')");
    }

    // Method 2: direct typing
    match enigo.text("[TYPE_OK]") {
        Ok(_) => println!("typed directly (expect '[TYPE_OK]')"),
        Err(e) => println!("direct typing FAILED: {e}"),
    }

    println!("\nDone. Tell me which markers appeared in the field.");
}
