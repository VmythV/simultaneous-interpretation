//! 前端可调用的命令

use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tingyi_core::audio::mic::InputDeviceInfo;
use tingyi_core::pipeline::PipelineEvent;
use tingyi_core::translate::{TranslateRequest, lang, openai_chat};

use crate::history::{self, Entry, Format, Record, Summary};
use crate::presets::{self, Presets};
use crate::session::{self, SessionState};
use crate::settings::Settings;
use crate::state::{AppState, UiState};

type CmdResult<T> = Result<T, String>;

const SAMPLE_WAV: &[u8] = include_bytes!("../../crates/core/tests/fixtures/two_sentences.wav");

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    /// 非致命问题，如快捷键注册失败
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<SaveResult> {
    let old = {
        let mut guard = state.settings.lock().unwrap();
        std::mem::replace(&mut *guard, settings.clone())
    };
    settings.save(&state.settings_path).map_err(|e| format!("保存设置失败：{e}"))?;
    if old.target_lang != settings.target_lang || old.fallback_lang != settings.fallback_lang {
        session::update_languages(&state, &settings.target_lang, &settings.fallback_lang).await;
    }
    if old.subtitle.always_on_top != settings.subtitle.always_on_top {
        crate::subtitle::apply_on_top(&app, settings.subtitle.always_on_top);
    }
    let warnings = if old.hotkeys != settings.hotkeys {
        crate::shortcuts::apply(&app, &settings.hotkeys)
    } else {
        state.hotkey_errors.lock().unwrap().clone()
    };
    let _ = app.emit("settings-changed", &settings);
    crate::tray::refresh(&app);
    Ok(SaveResult { warnings })
}

#[tauri::command]
pub fn get_hotkey_errors(state: State<'_, AppState>) -> Vec<String> {
    state.hotkey_errors.lock().unwrap().clone()
}

/// 录制新快捷键时暂停全局快捷键（否则按下的组合键会被全局快捷键截走），结束后恢复
#[tauri::command]
pub fn suspend_hotkeys(app: AppHandle, state: State<'_, AppState>, suspend: bool) -> Vec<String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if suspend {
        let _ = app.global_shortcut().unregister_all();
        state.hotkeys.lock().unwrap().clear();
        Vec::new()
    } else {
        crate::shortcuts::apply(&app, &state.settings().hotkeys)
    }
}

#[tauri::command]
pub fn get_presets() -> Presets {
    presets::all()
}

#[tauri::command]
pub async fn list_input_devices() -> CmdResult<Vec<InputDeviceInfo>> {
    tauri::async_runtime::spawn_blocking(tingyi_core::audio::mic::list_input_devices)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_session_state(state: State<'_, AppState>) -> CmdResult<SessionState> {
    Ok(if session::is_running(&state).await { SessionState::Running } else { SessionState::Idle })
}

#[tauri::command]
pub async fn start_session(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    session::start(&app, &state).await
}

#[tauri::command]
pub async fn stop_session(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    session::stop(&app, &state).await;
    Ok(())
}

/// 权限状态：granted / denied / undetermined / unknown。kind：mic | system
#[tauri::command]
pub async fn check_permission(kind: String) -> CmdResult<String> {
    permission(kind, false).await
}

/// 请求权限（会弹系统授权框，等待用户选择）
#[tauri::command]
pub async fn request_permission(kind: String) -> CmdResult<String> {
    permission(kind, true).await
}

async fn permission(kind: String, request: bool) -> CmdResult<String> {
    #[cfg(target_os = "macos")]
    {
        use tingyi_core::audio::system_macos as sys;
        tauri::async_runtime::spawn_blocking(move || match (kind.as_str(), request) {
            ("mic", false) => sys::check_mic_permission(),
            ("mic", true) => sys::request_mic_permission(),
            ("system", false) => sys::check_permission(),
            ("system", true) => sys::request_permission(),
            _ => Ok("unknown".into()),
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (kind, request);
        Ok("granted".into())
    }
}

#[tauri::command]
pub fn open_privacy_settings(kind: String) -> CmdResult<()> {
    let anchor = if kind == "mic" { "Privacy_Microphone" } else { "Privacy_ScreenCapture" };
    open_url(format!("x-apple.systempreferences:com.apple.preference.security?{anchor}"))
}

#[tauri::command]
pub fn open_url(url: String) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("x-apple.systempreferences:")) {
        return Err("不支持的链接".into());
    }
    std::process::Command::new("open").arg(&url).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub text: String,
    pub ms: u64,
}

/// 用内置的中文示例音频测试语音识别（使用传入的、尚未保存的设置）
#[tauri::command]
pub async fn test_stt(state: State<'_, AppState>, settings: Settings) -> CmdResult<TestResult> {
    let cfg = settings.stt_config()?;
    let samples = tingyi_core::wav::read_bytes(SAMPLE_WAV).map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let text = tingyi_core::stt::transcribe_once(&state.http, &cfg, &samples).await.map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Err("识别结果为空，请检查模型名称是否正确".into());
    }
    Ok(TestResult { text, ms: t0.elapsed().as_millis() as u64 })
}

/// 翻译一句示例，测试翻译服务（使用传入的、尚未保存的设置）
#[tauri::command]
pub async fn test_translate(state: State<'_, AppState>, settings: Settings) -> CmdResult<TestResult> {
    let llm = settings.llm_config()?;
    let target = if settings.target_lang == "zh" { settings.fallback_lang.clone() } else { settings.target_lang.clone() };
    let req = TranslateRequest { text: "你好，很高兴认识你。".into(), target_lang: target, context: Vec::new() };
    let t0 = Instant::now();
    let text = openai_chat::translate_stream(&state.http, &llm, &req, |_| {}).await.map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Err("翻译结果为空，请检查模型名称是否正确".into());
    }
    Ok(TestResult { text, ms: t0.elapsed().as_millis() as u64 })
}

/// 重新翻译一句（翻译失败后的"重试"），结果通过 pipeline 事件返回
#[tauri::command]
pub async fn retranslate(app: AppHandle, state: State<'_, AppState>, utt_id: u64, text: String) -> CmdResult<()> {
    let settings = state.settings();
    let llm = settings.llm_config()?;
    let target = lang::choose_target(lang::detect(&text).as_deref(), &settings.target_lang, &settings.fallback_lang);
    let http = state.http.clone();
    tauri::async_runtime::spawn(async move {
        let emit = |ev: PipelineEvent| {
            app.state::<AppState>().history.on_event(&ev);
            let _ = app.emit("pipeline", &ev);
        };
        emit(PipelineEvent::TrStart { utt_id, target_lang: target.clone() });
        let req = TranslateRequest { text, target_lang: target, context: Vec::new() };
        let t0 = Instant::now();
        let mut first = None;
        let res = openai_chat::translate_stream(&http, &llm, &req, |delta| {
            first.get_or_insert_with(|| t0.elapsed());
            emit(PipelineEvent::TrDelta { utt_id, delta: delta.to_string() });
        })
        .await;
        match res {
            Ok(text) => {
                let ms = t0.elapsed().as_millis() as u64;
                let ttft = first.map(|d| d.as_millis() as u64).unwrap_or(ms);
                emit(PipelineEvent::TrDone { utt_id, text, ttft_ms: ttft, speech_to_first_ms: 0, speech_to_done_ms: 0 });
            }
            Err(e) => emit(PipelineEvent::TrError { utt_id, message: e.to_string() }),
        }
    });
    Ok(())
}

/// 打开（或聚焦）设置窗口。tab：general | stt | translate
#[tauri::command]
pub async fn open_settings(app: AppHandle, tab: Option<String>) -> CmdResult<()> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        if let Some(tab) = tab {
            let _ = win.emit("settings-tab", tab);
        }
        return Ok(());
    }
    let url = match tab {
        Some(t) => format!("index.html#/settings/{t}"),
        None => "index.html#/settings".into(),
    };
    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App(url.into()))
        .title("设置")
        .inner_size(680.0, 580.0)
        .min_inner_size(600.0, 480.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ───────────── 悬浮字幕 ─────────────

#[tauri::command]
pub fn get_ui_state(state: State<'_, AppState>) -> UiState {
    state.ui()
}

#[tauri::command]
pub fn set_subtitle_visible(app: AppHandle, visible: bool) -> CmdResult<()> {
    crate::subtitle::set_visible(&app, visible)
}

#[tauri::command]
pub fn set_click_through(app: AppHandle, on: bool) {
    crate::subtitle::set_click_through(&app, on);
}

#[tauri::command]
pub fn show_main(app: AppHandle) {
    crate::tray::show_main(&app);
}

// ───────────── 历史记录 ─────────────

/// 打开（或聚焦）历史记录窗口
#[tauri::command]
pub async fn open_history(app: AppHandle) -> CmdResult<()> {
    app.state::<AppState>().history.flush();
    if let Some(win) = app.get_webview_window("history") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.emit("history-changed", ());
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "history", WebviewUrl::App("index.html#/history".into()))
        .title("历史记录")
        .inner_size(900.0, 620.0)
        .min_inner_size(680.0, 420.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_history(state: State<'_, AppState>) -> Vec<Summary> {
    state.history.flush();
    state.history.list()
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>, id: String) -> CmdResult<Record> {
    state.history.get(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_history(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.history.delete(&id).map_err(|e| e.to_string())?;
    let _ = app.emit("history-changed", ());
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state.history.clear().map_err(|e| e.to_string())?;
    let _ = app.emit("history-changed", ());
    Ok(())
}

/// 弹出保存对话框并写入文件。返回保存的路径；用户取消时返回 None
async fn save_with_dialog(app: &AppHandle, file_name: &str, format: Format, content: String) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let name: String = file_name.chars().map(|c| if "/\\:".contains(c) { '-' } else { c }).collect();
    app.dialog()
        .file()
        .set_title("导出")
        .set_file_name(format!("{name}.{}", format.ext()))
        .add_filter(format.filter_name(), &[format.ext()])
        .save_file(move |p| {
            let _ = tx.send(p);
        });
    let Some(path) = rx.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| format!("写入文件失败：{e}"))?;
    Ok(Some(path.display().to_string()))
}

/// 导出一条历史记录。title 由前端按本地时间格式化
#[tauri::command]
pub async fn export_history(app: AppHandle, state: State<'_, AppState>, id: String, format: Format, title: String) -> CmdResult<Option<String>> {
    let record = state.history.get(&id).map_err(|e| e.to_string())?;
    let content = history::render(&title, &record.entries, format);
    save_with_dialog(&app, &title, format, content).await
}

/// 导出主窗口当前显示的内容
#[tauri::command]
pub async fn export_entries(app: AppHandle, entries: Vec<Entry>, format: Format, title: String) -> CmdResult<Option<String>> {
    let content = history::render(&title, &entries, format);
    save_with_dialog(&app, &title, format, content).await
}

/// 在访达中显示文件
#[tauri::command]
pub fn reveal_file(path: String) -> CmdResult<()> {
    std::process::Command::new("open").arg("-R").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
