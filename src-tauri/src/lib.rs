//! 听译桌面应用（Tauri 外壳）：设置、会话管理、窗口、菜单栏、快捷键、历史；核心逻辑在 tingyi-core。

mod commands;
mod history;
mod presets;
mod session;
mod settings;
mod shortcuts;
mod state;
mod subtitle;
mod tray;

use std::time::Duration;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::state::AppState;

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(shortcuts::plugin())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            app.manage(AppState::new(config_dir));
            let handle = app.handle().clone();

            tray::create(&handle)?;
            let hotkeys = handle.state::<AppState>().settings().hotkeys;
            let errors = shortcuts::apply(&handle, &hotkeys);
            for e in errors {
                eprintln!("{e}");
            }
            subtitle::restore(&handle);

            // 历史记录每 3 秒落盘一次（有改动时）
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    h.state::<AppState>().history.flush();
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| match window.label() {
            // 关闭主窗口只是隐藏：软件继续在菜单栏运行（字幕也继续显示）；退出用菜单栏或 ⌘Q
            "main" => {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            subtitle::LABEL => subtitle::on_window_event(window, event),
            // 设置窗口关闭时重新注册快捷键（防止录制快捷键时关窗，快捷键一直处于暂停状态）
            "settings" => {
                if let WindowEvent::Destroyed = event {
                    let app = window.app_handle();
                    shortcuts::apply(app, &app.state::<AppState>().settings().hotkeys);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_hotkey_errors,
            commands::suspend_hotkeys,
            commands::get_presets,
            commands::list_input_devices,
            commands::list_audio_apps,
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
            commands::get_ui_state,
            commands::set_subtitle_visible,
            commands::set_click_through,
            commands::show_main,
            commands::open_history,
            commands::list_history,
            commands::get_history,
            commands::delete_history,
            commands::clear_history,
            commands::export_history,
            commands::export_entries,
            commands::reveal_file,
        ])
        .build(tauri::generate_context!())
        .expect("启动听译失败");

    app.run(|app, event| match event {
        // 点击 Dock 图标：重新显示主窗口
        RunEvent::Reopen { .. } => tray::show_main(app),
        // 退出前把未写入的历史记录落盘。麦克风随进程关闭，电脑声音辅助程序在 stdin 关闭后自行退出
        RunEvent::Exit => app.state::<AppState>().history.end(),
        _ => {}
    });
}
