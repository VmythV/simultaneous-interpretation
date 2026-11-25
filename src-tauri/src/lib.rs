//! 听译桌面应用（Tauri 外壳）：设置、会话管理、窗口；核心逻辑在 tingyi-core。

mod commands;
mod presets;
mod session;
mod settings;

use tauri::{Manager, WindowEvent};

use crate::session::AppState;
use crate::settings::Settings;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let settings = Settings::load(&path);
            app.manage(AppState::new(settings, path));
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭主窗口即退出（连同设置窗口）
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_presets,
            commands::list_input_devices,
            commands::get_session_state,
            commands::start_session,
            commands::stop_session,
            commands::check_permission,
            commands::request_permission,
            commands::open_privacy_settings,
            commands::open_url,
            commands::test_stt,
            commands::test_translate,
            commands::retranslate,
            commands::open_settings,
        ])
        // 退出时不需要专门停止会话：麦克风随进程关闭，电脑声音辅助程序在 stdin 关闭后自行退出
        .run(tauri::generate_context!())
        .expect("启动听译失败");
}
