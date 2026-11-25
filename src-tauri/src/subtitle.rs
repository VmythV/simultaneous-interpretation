//! 悬浮字幕窗口：透明、置顶、无边框，可拖动；可切换鼠标穿透

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};

use crate::state::{AppState, Bounds};

pub const LABEL: &str = "subtitle";

fn default_bounds(app: &AppHandle) -> Bounds {
    let (sw, sh) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let s = m.size().to_logical::<f64>(m.scale_factor());
            (s.width, s.height)
        })
        .unwrap_or((1440.0, 900.0));
    let width = (sw * 0.6).clamp(480.0, 1000.0);
    let height = 150.0;
    Bounds { x: (sw - width) / 2.0, y: sh - height - 110.0, width, height }
}

fn create(app: &AppHandle, state: &AppState) -> Result<WebviewWindow, String> {
    let ui = state.ui();
    let b = ui.subtitle_bounds.unwrap_or_else(|| default_bounds(app));
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#/subtitle".into()))
        .title("听译字幕")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(true)
        .focused(false)
        .position(b.x, b.y)
        .inner_size(b.width, b.height)
        .min_inner_size(320.0, 70.0)
        .build()
        .map_err(|e| format!("创建字幕窗口失败：{e}"))?;
    if ui.click_through {
        let _ = win.set_ignore_cursor_events(true);
    }
    Ok(win)
}

pub fn set_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    if visible {
        let win = match app.get_webview_window(LABEL) {
            Some(w) => w,
            None => create(app, &state)?,
        };
        win.show().map_err(|e| e.to_string())?;
    } else if let Some(w) = app.get_webview_window(LABEL) {
        w.hide().map_err(|e| e.to_string())?;
    }
    state.update_ui(app, |ui| ui.subtitle_visible = visible);
    Ok(())
}

pub fn toggle(app: &AppHandle) {
    let visible = app.state::<AppState>().ui().subtitle_visible;
    let _ = set_visible(app, !visible);
}

pub fn set_click_through(app: &AppHandle, on: bool) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_ignore_cursor_events(on);
    }
    app.state::<AppState>().update_ui(app, |ui| ui.click_through = on);
}

pub fn toggle_click_through(app: &AppHandle) {
    let on = app.state::<AppState>().ui().click_through;
    // 打开穿透时如果字幕没显示，先显示出来，否则用户看不到变化
    if !on && !app.state::<AppState>().ui().subtitle_visible {
        let _ = set_visible(app, true);
    }
    set_click_through(app, !on);
}

/// 启动时恢复上次的显示状态
pub fn restore(app: &AppHandle) {
    if app.state::<AppState>().ui().subtitle_visible {
        let _ = set_visible(app, true);
    }
}

/// 记住字幕窗口的位置和大小
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if !matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
        return;
    }
    let (Ok(sf), Ok(pos), Ok(size)) = (window.scale_factor(), window.outer_position(), window.inner_size()) else { return };
    if size.width == 0 || size.height == 0 {
        return;
    }
    let pos = pos.to_logical::<f64>(sf);
    let size = size.to_logical::<f64>(sf);
    let app = window.app_handle();
    app.state::<AppState>().update_ui(app, |ui| {
        ui.subtitle_bounds = Some(Bounds { x: pos.x, y: pos.y, width: size.width, height: size.height });
    });
}
