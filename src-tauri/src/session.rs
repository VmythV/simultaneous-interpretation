//! 会话：一次"开始 → 停止"。持有流水线和音源采集，把流水线事件转发给前端。

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tingyi_core::audio::{self, CaptureHandle, CaptureSender};
use tingyi_core::pipeline::Pipeline;
use tokio::sync::mpsc;

use crate::settings::{Settings, Source};

pub struct Session {
    pipeline: Pipeline,
    captures: Vec<CaptureHandle>,
    forward: tauri::async_runtime::JoinHandle<()>,
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub session: tokio::sync::Mutex<Option<Session>>,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(settings: Settings, settings_path: PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("创建 HTTP 客户端失败");
        Self { settings: Mutex::new(settings), settings_path, session: tokio::sync::Mutex::new(None), http }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Idle,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvent {
    pub state: SessionState,
    pub source: Option<Source>,
    pub error: Option<String>,
}

pub fn emit_state(app: &AppHandle, state: SessionState, source: Option<Source>, error: Option<String>) {
    let _ = app.emit("session", SessionEvent { state, source, error });
}

fn start_capture(source: Source, mic_device: String, tx: CaptureSender) -> anyhow::Result<Vec<CaptureHandle>> {
    let handle = match source {
        Source::Mic => audio::mic::start(Some(mic_device.as_str()).filter(|d| !d.is_empty()), tx)?,
        #[cfg(target_os = "macos")]
        Source::System => audio::system_macos::start(tx)?,
        #[cfg(not(target_os = "macos"))]
        Source::System => anyhow::bail!("电脑声音采集暂时只支持 macOS"),
    };
    Ok(vec![handle])
}

pub async fn start(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let mut guard = state.session.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    let settings = state.settings();
    let source = settings.source;
    let cfg = settings.pipeline_config()?;
    emit_state(app, SessionState::Starting, Some(source), None);

    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel();
    let pipeline = Pipeline::start(cfg, ev_tx).map_err(|e| e.to_string())?;
    let sender = pipeline.sender();
    let mic = settings.mic_device.clone();
    // 打开麦克风可能会等待系统授权弹窗，放到阻塞线程里
    let captures = match tauri::async_runtime::spawn_blocking(move || start_capture(source, mic, sender)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            pipeline.shutdown().await;
            let msg = e.to_string();
            emit_state(app, SessionState::Idle, Some(source), Some(msg.clone()));
            return Err(msg);
        }
        Err(e) => {
            pipeline.shutdown().await;
            emit_state(app, SessionState::Idle, Some(source), Some(e.to_string()));
            return Err(e.to_string());
        }
    };

    let app2 = app.clone();
    let forward = tauri::async_runtime::spawn(async move {
        while let Some(ev) = ev_rx.recv().await {
            let _ = app2.emit("pipeline", &ev);
        }
    });
    *guard = Some(Session { pipeline, captures, forward });
    emit_state(app, SessionState::Running, Some(source), None);
    Ok(())
}

pub async fn stop(app: &AppHandle, state: &AppState) {
    let session = state.session.lock().await.take();
    let Some(Session { pipeline, captures, forward }) = session else {
        emit_state(app, SessionState::Idle, None, None);
        return;
    };
    emit_state(app, SessionState::Stopping, None, None);
    // 停止采集会等待辅助程序退出（最多约 1 秒），放到阻塞线程里
    let _ = tauri::async_runtime::spawn_blocking(move || drop(captures)).await;
    pipeline.shutdown().await;
    let _ = forward.await;
    emit_state(app, SessionState::Idle, None, None);
}

pub async fn is_running(state: &AppState) -> bool {
    state.session.lock().await.is_some()
}

/// 运行中修改翻译语言
pub async fn update_languages(state: &AppState, target: &str, fallback: &str) {
    if let Some(s) = state.session.lock().await.as_ref() {
        s.pipeline.set_languages(target, fallback);
    }
}
