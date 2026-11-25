//! 音频采集。所有音源最终都输出 16kHz 单声道 f32 帧。

pub mod mic;
pub mod resample;
#[cfg(target_os = "macos")]
pub mod system_macos;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

/// 识别、VAD 统一使用的采样率
pub const TARGET_RATE: u32 = 16_000;

/// 音源通道：会议模式下用来区分"我"和"对方"
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Mic,
    System,
}

impl Channel {
    pub fn label(self) -> &'static str {
        match self {
            Channel::Mic => "我",
            Channel::System => "对方",
        }
    }
}

/// 采集线程发出的消息
#[derive(Debug)]
pub enum CaptureMsg {
    /// 16kHz 单声道音频
    Audio { channel: Channel, samples: Vec<f32> },
    /// 状态信息（设备名、权限等）
    Info { channel: Channel, message: String },
    /// 采集出错，该通道已停止
    Error { channel: Channel, message: String },
}

pub type CaptureSender = tokio::sync::mpsc::UnboundedSender<CaptureMsg>;

/// 采集句柄：drop 时停止采集
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    on_stop: Option<Box<dyn FnOnce() + Send>>,
}

impl CaptureHandle {
    pub(crate) fn new(stop: Arc<AtomicBool>, on_stop: Option<Box<dyn FnOnce() + Send>>) -> Self {
        Self { stop, on_stop }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(f) = self.on_stop.take() {
            f();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 计算一段音频的 RMS 音量
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}
