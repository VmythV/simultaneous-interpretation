//! 菜单栏图标：开始/停止、切换音源、字幕开关、打开窗口、退出

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::session::SessionState;
use crate::settings::Source;
use crate::state::AppState;

const ICON_IDLE: &[u8] = include_bytes!("../icons/tray.png");
const ICON_ACTIVE: &[u8] = include_bytes!("../icons/tray-active.png");

pub struct TrayHandles {
    tray: TrayIcon,
    toggle: MenuItem<Wry>,
    src_mic: CheckMenuItem<Wry>,
    src_system: CheckMenuItem<Wry>,
    src_both: CheckMenuItem<Wry>,
    subtitle: CheckMenuItem<Wry>,
    click_through: CheckMenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "开始", true, None::<&str>)?;
    let src_mic = CheckMenuItem::with_id(app, "source-mic", "听麦克风", true, false, None::<&str>)?;
    let src_system = CheckMenuItem::with_id(app, "source-system", "听电脑声音", true, false, None::<&str>)?;
    let src_both = CheckMenuItem::with_id(app, "source-both", "会议（麦克风 + 电脑声音）", true, false, None::<&str>)?;
    let subtitle = CheckMenuItem::with_id(app, "subtitle", "显示悬浮字幕", true, false, None::<&str>)?;
    let click_through = CheckMenuItem::with_id(app, "click-through", "字幕鼠标穿透", true, false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show-main", "打开主窗口", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "历史记录…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出听译", true, None::<&str>)?;
    let (s1, s2, s3, s4) = (
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
    );
    let menu = Menu::with_items(
        app,
        &[&toggle, &s1, &src_mic, &src_system, &src_both, &s2, &subtitle, &click_through, &s3, &show, &history, &settings, &s4, &quit],
    )?;
    let tray = TrayIconBuilder::with_id("tingyi")
        .icon(Image::from_bytes(ICON_IDLE)?)
        .icon_as_template(true)
        .tooltip("听译")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu)
        .build(app)?;

    *app.state::<AppState>().tray.lock().unwrap() =
        Some(TrayHandles { tray, toggle, src_mic, src_system, src_both, subtitle, click_through });
    refresh(app);
    Ok(())
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "toggle" => request_toggle(app),
        "source-mic" => request_source(app, Source::Mic),
        "source-system" => request_source(app, Source::System),
        "source-both" => request_source(app, Source::Both),
        "subtitle" => crate::subtitle::toggle(app),
        "click-through" => crate::subtitle::toggle_click_through(app),
        "show-main" => show_main(app),
        "history" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::commands::open_history(app).await;
            });
        }
        "settings" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::commands::open_settings(app, None).await;
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
    // CheckMenuItem 被点击时会自己切换勾选，统一按真实状态刷新
    refresh(app);
}

/// 开始/停止交给主窗口处理（那里有权限检查流程）
pub fn request_toggle(app: &AppHandle) {
    let _ = app.emit_to("main", "request-toggle", ());
}

/// 切换音源交给主窗口处理（运行中需要重新开始）
pub fn request_source(app: &AppHandle, source: Source) {
    let _ = app.emit_to("main", "request-source", source);
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 按当前状态刷新菜单文字、勾选和图标
pub fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let session = *state.session_state.lock().unwrap();
    let source = state.settings.lock().unwrap().source;
    let ui = state.ui();
    let guard = state.tray.lock().unwrap();
    let Some(t) = guard.as_ref() else { return };
    let running = matches!(session, SessionState::Running | SessionState::Starting);
    let _ = t.toggle.set_text(match session {
        SessionState::Running => "停止",
        SessionState::Starting => "准备中…",
        SessionState::Stopping => "正在停止…",
        SessionState::Idle => "开始",
    });
    let _ = t.src_mic.set_checked(source == Source::Mic);
    let _ = t.src_system.set_checked(source == Source::System);
    let _ = t.src_both.set_checked(source == Source::Both);
    let _ = t.subtitle.set_checked(ui.subtitle_visible);
    let _ = t.click_through.set_checked(ui.click_through);
    if let Ok(img) = Image::from_bytes(if running { ICON_ACTIVE } else { ICON_IDLE }) {
        let _ = t.tray.set_icon(Some(img));
        let _ = t.tray.set_icon_as_template(true);
    }
    let _ = t.tray.set_tooltip(Some(if running { "听译 · 正在听" } else { "听译" }));
}
