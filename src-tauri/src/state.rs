//! 应用全局状态

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::history::Recorder;
use crate::session::{Session, SessionState};
use crate::settings::Settings;
use crate::shortcuts::Action;
use crate::tray::TrayHandles;

/// 窗口等界面状态（后端维护，不放进用户设置，避免设置窗口保存时覆盖）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiState {
    pub subtitle_visible: bool,
    pub subtitle_bounds: Option<Bounds>,
    /// 字幕鼠标穿透（不持久化，每次启动都是关闭）
    pub click_through: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl UiState {
    pub fn load(path: &PathBuf) -> Self {
        let mut s: Self = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        s.click_through = false;
        s
    }
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub ui: Mutex<UiState>,
    ui_path: PathBuf,
    ui_save_gen: AtomicU64,
    pub session: tokio::sync::Mutex<Option<Session>>,
    pub session_state: Mutex<SessionState>,
    pub http: reqwest::Client,
    pub tray: Mutex<Option<TrayHandles>>,
    pub hotkeys: Mutex<HashMap<u32, Action>>,
    pub hotkey_errors: Mutex<Vec<String>>,
    pub history: Recorder,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Self {
        let settings_path = config_dir.join("settings.json");
        let ui_path = config_dir.join("ui-state.json");
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("创建 HTTP 客户端失败");
        Self {
            settings: Mutex::new(Settings::load(&settings_path)),
            settings_path,
            ui: Mutex::new(UiState::load(&ui_path)),
            ui_path,
            ui_save_gen: AtomicU64::new(0),
            session: tokio::sync::Mutex::new(None),
            session_state: Mutex::new(SessionState::Idle),
            http,
            tray: Mutex::new(None),
            hotkeys: Mutex::new(HashMap::new()),
            hotkey_errors: Mutex::new(Vec::new()),
            history: Recorder::new(config_dir.join("history")),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn ui(&self) -> UiState {
        self.ui.lock().unwrap().clone()
    }

    /// 修改界面状态：通知各窗口、刷新菜单栏，并在 0.5 秒后写盘（拖动窗口时会连续触发）
    pub fn update_ui(&self, app: &AppHandle, f: impl FnOnce(&mut UiState)) {
        let snapshot = {
            let mut ui = self.ui.lock().unwrap();
            f(&mut ui);
            ui.clone()
        };
        let _ = app.emit("ui-state", &snapshot);
        crate::tray::refresh(app);

        let generation = self.ui_save_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let state = app.state::<AppState>();
            if state.ui_save_gen.load(Ordering::SeqCst) != generation {
                return;
            }
            let ui = state.ui();
            if let Ok(json) = serde_json::to_string_pretty(&ui) {
                let _ = std::fs::write(&state.ui_path, json);
            }
        });
    }
}
