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
    let on_top = state.settings.lock().unwrap().subtitle.always_on_top;
    let b = ui.subtitle_bounds.unwrap_or_else(|| default_bounds(app));
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#/subtitle".into()))
        .title("听译字幕")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(on_top)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(true)
        // 双击拖动区域时不要放大到全屏
        .maximizable(false)
        .focused(false)
        // 字幕窗口不是当前活动窗口时（例如刚点了主窗口的「开始」），第一次点击也直接交给页面，
        // 否则 macOS 会把这次点击用来激活窗口，导致按住拖不动
        .accept_first_mouse(true)
        .position(b.x, b.y)
        .inner_size(b.width, b.height)
        .min_inner_size(320.0, 70.0)
        .build()
        .map_err(|e| format!("创建字幕窗口失败：{e}"))?;
    if ui.click_through {
        let _ = win.set_ignore_cursor_events(true);
    }
    apply_level(&win, on_top);
    Ok(win)
}

/// 设置窗口层级。置顶时用"状态栏"层级（高于普通和浮动窗口），并允许出现在全屏应用所在的桌面空间里
fn apply_level(win: &WebviewWindow, on_top: bool) {
    #[cfg(target_os = "macos")]
    {
        let w = win.clone();
        let _ = win.run_on_main_thread(move || {
            let Ok(ptr) = w.ns_window() else { return };
            let ns = ptr.cast::<objc2::runtime::AnyObject>();
            // NSStatusWindowLevel = 25，普通窗口 = 0
            let level: isize = if on_top { 25 } else { 0 };
            // CanJoinAllSpaces | Stationary | IgnoresCycle | FullScreenAuxiliary
            let behavior: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
            // SAFETY: 在主线程上对有效的 NSWindow 调用 setLevel: / setCollectionBehavior:
            unsafe {
                let _: () = objc2::msg_send![ns, setLevel: level];
                let _: () = objc2::msg_send![ns, setCollectionBehavior: behavior];
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win.set_always_on_top(on_top);
    }
}

/// 设置里修改"始终置顶"后调用
pub fn apply_on_top(app: &AppHandle, on_top: bool) {
    if let Some(w) = app.get_webview_window(LABEL) {
        apply_level(&w, on_top);
    }
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
