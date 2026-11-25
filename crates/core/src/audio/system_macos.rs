//! macOS 电脑声音采集：启动 Swift 辅助程序 audio-tap，从它的 stdout 读取 f32le 单声道 PCM。

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use super::resample::ToTarget;
use super::{CaptureHandle, CaptureMsg, CaptureSender, Channel};

const HELPER_NAME: &str = "audio-tap";

/// 查找辅助程序：环境变量 TINGYI_AUDIO_TAP → 与当前程序同目录 → 上一级目录（target/）
pub fn helper_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TINGYI_AUDIO_TAP") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join(HELPER_NAME), dir.join("..").join(HELPER_NAME)].into_iter().find(|p| p.is_file())
}

fn require_helper() -> Result<PathBuf> {
    helper_path().ok_or_else(|| anyhow!("找不到 {HELPER_NAME}，请先运行 scripts/build-audio-tap.sh"))
}

fn run_once(arg: &str) -> Result<Value> {
    let out = Command::new(require_helper()?).arg(arg).output().context("运行 audio-tap 失败")?;
    let line = String::from_utf8_lossy(&out.stderr);
    let line = line.lines().last().unwrap_or_default();
    serde_json::from_str(line).with_context(|| format!("无法解析 audio-tap 输出：{line}"))
}

/// 检查"系统录音"权限：granted / denied / undetermined / unknown
pub fn check_permission() -> Result<String> {
    Ok(run_once("--check")?["permission"].as_str().unwrap_or("unknown").to_string())
}

/// 请求"系统录音"权限（会弹系统授权框并等待用户选择）
pub fn request_permission() -> Result<String> {
    Ok(run_once("--request")?["permission"].as_str().unwrap_or("unknown").to_string())
}

/// 开始采集电脑声音。音频以 16kHz 单声道发送到 `tx`。
pub fn start(tx: CaptureSender) -> Result<CaptureHandle> {
    start_with(&require_helper()?, tx)
}

pub fn start_with(helper: &Path, tx: CaptureSender) -> Result<CaptureHandle> {
    let mut child = Command::new(helper)
        .arg("--watch-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("启动 {} 失败", helper.display()))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let (rate_tx, rate_rx) = mpsc::channel::<Result<u32>>();

    // stderr：JSON 事件
    let err_tx = tx.clone();
    std::thread::Builder::new().name("audio-tap-events".into()).spawn(move || {
        let mut rate_tx = Some(rate_tx);
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let Ok(ev) = serde_json::from_str::<Value>(&line) else { continue };
            match ev["event"].as_str() {
                Some("format") => {
                    if let Some(t) = rate_tx.take() {
                        let _ = t.send(ev["sampleRate"].as_u64().map(|r| r as u32).ok_or_else(|| anyhow!("缺少采样率")));
                    }
                }
                Some("started") => {
                    let perm = ev["permission"].as_str().unwrap_or("unknown");
                    let mut message = format!("电脑声音：已开始（系统录音权限：{perm}）");
                    if perm != "granted" {
                        message.push_str("；如果一直没有声音，请在 系统设置 → 隐私与安全性 → 屏幕与系统录音 中授权");
                    }
                    let _ = err_tx.send(CaptureMsg::Info { channel: Channel::System, message });
                }
                Some("error") => {
                    let message = format!(
                        "电脑声音采集失败：{}（{}，code={}）",
                        ev["message"].as_str().unwrap_or("?"),
                        ev["stage"].as_str().unwrap_or("?"),
                        ev["code"]
                    );
                    if let Some(t) = rate_tx.take() {
                        let _ = t.send(Err(anyhow!(message.clone())));
                    }
                    let _ = err_tx.send(CaptureMsg::Error { channel: Channel::System, message });
                }
                _ => {}
            }
        }
    })?;

    let rate = match rate_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            let _ = child.kill();
            return Err(e);
        }
        Err(_) => {
            let _ = child.kill();
            bail!("audio-tap 启动超时");
        }
    };
    let mut resampler = ToTarget::new(rate)?;

    // stdout：PCM
    let stop2 = stop.clone();
    std::thread::Builder::new().name("audio-tap-pcm".into()).spawn(move || {
        let mut reader = stdout;
        let mut buf = vec![0u8; 4096 * 4];
        let mut carry: Vec<u8> = Vec::new();
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            carry.extend_from_slice(&buf[..n]);
            let whole = carry.len() / 4 * 4;
            let samples: Vec<f32> =
                carry[..whole].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            carry.drain(..whole);
            match resampler.push(&samples) {
                Ok(out) if !out.is_empty() => {
                    if tx.send(CaptureMsg::Audio { channel: Channel::System, samples: out }).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = tx.send(CaptureMsg::Error { channel: Channel::System, message: e.to_string() });
                    break;
                }
            }
        }
        if !stop2.load(Ordering::SeqCst) {
            let _ = tx.send(CaptureMsg::Error { channel: Channel::System, message: "audio-tap 意外退出".into() });
        }
    })?;

    let child = Arc::new(Mutex::new(Some(child)));
    let on_stop = Box::new(move || {
        if let Some(mut c) = child.lock().unwrap().take() {
            terminate(&mut c);
        }
    });
    Ok(CaptureHandle::new(stop, Some(on_stop)))
}

/// 关闭 stdin 让辅助程序自行清理退出；1 秒内没退出就强制结束
fn terminate(child: &mut Child) {
    drop(child.stdin.take());
    for _ in 0..20 {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}
