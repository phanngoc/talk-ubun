//! System-tray indicator (StatusNotifierItem via ksni).
//!
//! Shows recording state persistently in the top bar (unlike notifications,
//! which GNOME auto-dismisses) and lets you toggle by left-clicking the icon or
//! using the right-click menu. Requires an SNI host — Ubuntu's AppIndicator
//! extension provides one.

use std::sync::mpsc::Sender;

pub struct TrayApp {
    pub recording: bool,
    pub toggle_tx: Sender<()>,
}

impl ksni::Tray for TrayApp {
    fn id(&self) -> String {
        "talk-ubun".into()
    }

    fn title(&self) -> String {
        "talk-ubun".into()
    }

    fn icon_name(&self) -> String {
        if self.recording {
            "media-record".into()
        } else {
            "audio-input-microphone".into()
        }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: self.icon_name(),
            icon_pixmap: Vec::new(),
            title: "talk-ubun".into(),
            description: if self.recording {
                "🔴 Đang ghi âm — bấm để dừng".into()
            } else {
                "Sẵn sàng — bấm để ghi (hoặc dùng phím tắt)".into()
            },
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.toggle_tx.send(());
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: if self.recording {
                    "⏹  Dừng ghi".into()
                } else {
                    "🎙  Bắt đầu ghi".into()
                },
                activate: Box::new(|t: &mut TrayApp| {
                    let _ = t.toggle_tx.send(());
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Thoát".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Start the tray on a background thread and return a handle to update its
/// state. The handle's `update(...)` re-renders the icon/menu.
pub fn start(toggle_tx: Sender<()>) -> ksni::Handle<TrayApp> {
    let service = ksni::TrayService::new(TrayApp {
        recording: false,
        toggle_tx,
    });
    let handle = service.handle();
    service.spawn();
    handle
}
