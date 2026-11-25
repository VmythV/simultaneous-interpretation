//! 会话：一次"开始 → 停止"。持有流水线和音源采集，把流水线事件转发给前端并写入历史记录。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tingyi_core::audio::{self, CaptureHandle, CaptureSender};
use tingyi_core::pipeline::Pipeline;
use tokio::sync::mpsc;

use crate::settings::Source;
use crate::state::AppState;

pub struct Session {
    pipeline: Pipeline,
    captures: Vec<CaptureHandle>,
    forward: tauri::async_runtime::JoinHandle<()>,
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
    *app.state::<AppState>().session_state.lock().unwrap() = state;
    let _ = app.emit("session", SessionEvent { state, source, error });
    crate::tray::refresh(app);
}

struct CaptureOpts {
    mic_device: String,
    system_app: String,
    echo_cancel: bool,
}

fn start_capture(source: Source, opts: CaptureOpts, tx: CaptureSender) -> anyhow::Result<Vec<CaptureHandle>> {
    let mic = || audio::mic::start(Some(opts.mic_device.as_str()).filter(|d| !d.is_empty()), tx.clone());
    #[cfg(target_os = "macos")]
    let system = || audio::system_macos::start_app(Some(opts.system_app.as_str()), tx.clone());
    #[cfg(not(target_os = "macos"))]
    let system = || -> anyhow::Result<CaptureHandle> { anyhow::bail!("电脑声音采集暂时只支持 macOS") };
    Ok(match source {
        Source::Mic => vec![mic()?],
        Source::System => vec![system()?],
        Source::Both => {
            // 先启动麦克风：回声消除可能改变输出设备的格式，电脑声音要在它之后读取格式
            #[cfg(target_os = "macos")]
            let m = if opts.echo_cancel {
                match audio::system_macos::start_mic_aec(tx.clone()) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx.send(audio::CaptureMsg::Info {
                            channel: audio::Channel::Mic,
                            message: format!("系统回声消除启动失败，改用普通麦克风：{e}"),
                        });
                        mic()?
                    }
                }
            } else {
                mic()?
            };
            #[cfg(not(target_os = "macos"))]
            let m = mic()?;
            vec![m, system()?]
        }
    })
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
    let opts = CaptureOpts {
        mic_device: settings.mic_device.clone(),
        system_app: settings.system_app.clone(),
        echo_cancel: settings.echo_cancel,
    };
    // 打开麦克风可能会等待系统授权弹窗，放到阻塞线程里
    let captures = match tauri::async_runtime::spawn_blocking(move || start_capture(source, opts, sender)).await {
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

    state.history.begin(source, &settings.target_lang, settings.history.save);
    let app2 = app.clone();
    let forward = tauri::async_runtime::spawn(async move {
        while let Some(ev) = ev_rx.recv().await {
            app2.state::<AppState>().history.on_event(&ev);
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
    state.history.end();
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
