//! 全局快捷键：开始/停止、显示/隐藏字幕、字幕鼠标穿透

use std::collections::HashMap;

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::settings::HotkeySettings;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Toggle,
    Subtitle,
    ClickThrough,
}

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let action = app.state::<AppState>().hotkeys.lock().unwrap().get(&shortcut.id()).copied();
            match action {
                Some(Action::Toggle) => crate::tray::request_toggle(app),
                Some(Action::Subtitle) => crate::subtitle::toggle(app),
                Some(Action::ClickThrough) => crate::subtitle::toggle_click_through(app),
                None => {}
            }
        })
        .build()
}

/// 按设置重新注册全部快捷键，返回出错信息（如被其他软件占用）
pub fn apply(app: &AppHandle, h: &HotkeySettings) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut map = HashMap::new();
    let mut errors = Vec::new();
    for (action, accel, name) in [
        (Action::Toggle, &h.toggle, "开始/停止"),
        (Action::Subtitle, &h.subtitle, "显示/隐藏字幕"),
        (Action::ClickThrough, &h.click_through, "字幕鼠标穿透"),
    ] {
        let accel = accel.trim();
        if accel.is_empty() {
            continue;
        }
        match accel.parse::<Shortcut>() {
            Ok(sc) => match gs.register(sc) {
                Ok(()) => {
                    map.insert(sc.id(), action);
                }
                Err(e) => errors.push(format!("「{name}」的快捷键 {accel} 注册失败（可能被其他软件占用）：{e}")),
            },
            Err(e) => errors.push(format!("「{name}」的快捷键 {accel} 无效：{e}")),
        }
    }
    let state = app.state::<AppState>();
    *state.hotkeys.lock().unwrap() = map;
    *state.hotkey_errors.lock().unwrap() = errors.clone();
    errors
}
