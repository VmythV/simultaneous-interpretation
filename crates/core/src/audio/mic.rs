//! 麦克风采集（cpal，跨平台）

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use serde::Serialize;

use super::resample::ToTarget;
use super::{CaptureHandle, CaptureMsg, CaptureSender, Channel};

#[derive(Debug, Clone, Serialize)]
pub struct InputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
    let mut out = Vec::new();
    for device in host.input_devices().context("枚举输入设备失败")? {
        let id = device.id().map(|i| i.to_string()).unwrap_or_default();
        let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| id.clone());
        out.push(InputDeviceInfo { is_default: Some(&id) == default_id.as_ref(), id, name });
    }
    Ok(out)
}

fn find_device(selector: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match selector {
        None | Some("") | Some("default") => host.default_input_device().ok_or_else(|| anyhow!("没有找到默认麦克风")),
        Some(sel) => {
            for device in host.input_devices()? {
                let id = device.id().map(|i| i.to_string()).unwrap_or_default();
                let name = device.description().map(|d| d.name().to_string()).unwrap_or_default();
                if id == sel || name == sel {
                    return Ok(device);
                }
            }
            bail!("找不到麦克风设备：{sel}")
        }
    }
}

/// 开始采集麦克风。音频以 16kHz 单声道发送到 `tx`。
pub fn start(device: Option<&str>, tx: CaptureSender) -> Result<CaptureHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<String>>();
    let device = device.map(str::to_owned);
    let stop2 = stop.clone();
    let info_tx = tx.clone();

    // cpal 的 Stream 不一定能跨线程，所以创建、持有、处理都放在同一个专用线程里
    std::thread::Builder::new().name("mic-capture".into()).spawn(move || {
        let setup = || -> Result<(cpal::Stream, mpsc::Receiver<Vec<f32>>, u32, String)> {
            let device = find_device(device.as_deref())?;
            let name = device.description().map(|d| d.name().to_string()).unwrap_or_default();
            let supported = device.default_input_config().context("读取麦克风默认格式失败")?;
            let rate = supported.sample_rate();
            let channels = supported.channels() as usize;
            let (raw_tx, raw_rx) = mpsc::channel::<Vec<f32>>();
            let err_tx = tx.clone();
            let config = supported.config();
            let on_err = move |e: cpal::Error| {
                let _ = err_tx.send(CaptureMsg::Error { channel: Channel::Mic, message: format!("麦克风错误：{e}") });
            };
            let stream = match supported.sample_format() {
                SampleFormat::F32 => build::<f32>(&device, config, channels, raw_tx, on_err)?,
                SampleFormat::I16 => build::<i16>(&device, config, channels, raw_tx, on_err)?,
                SampleFormat::I32 => build::<i32>(&device, config, channels, raw_tx, on_err)?,
                other => bail!("不支持的麦克风采样格式：{other:?}"),
            };
            stream.play().context("启动麦克风失败")?;
            Ok((stream, raw_rx, rate, format!("{name}（{rate}Hz，{channels} 声道）")))
        };

        let (stream, raw_rx, rate, desc) = match setup() {
            Ok(v) => v,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return;
            }
        };
        let mut resampler = match ToTarget::new(rate) {
            Ok(r) => r,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return;
            }
        };
        let _ = ready_tx.send(Ok(desc));

        while !stop2.load(Ordering::SeqCst) {
            match raw_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => match resampler.push(&chunk) {
                    Ok(samples) if !samples.is_empty() => {
                        if tx.send(CaptureMsg::Audio { channel: Channel::Mic, samples }).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        let _ = tx.send(CaptureMsg::Error { channel: Channel::Mic, message: e.to_string() });
                        break;
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        drop(stream);
    })?;

    let desc = ready_rx.recv().map_err(|_| anyhow!("麦克风线程意外退出"))??;
    let _ = info_tx.send(CaptureMsg::Info { channel: Channel::Mic, message: format!("麦克风：{desc}") });
    Ok(CaptureHandle::new(stop, None))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    raw_tx: mpsc::Sender<Vec<f32>>,
    on_err: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: SizedSample + Sample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| {
            let mono: Vec<f32> = if channels == 1 {
                data.iter().map(|s| s.to_sample::<f32>()).collect()
            } else {
                data.chunks(channels)
                    .map(|frame| frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32)
                    .collect()
            };
            let _ = raw_tx.send(mono);
        },
        on_err,
        None,
    )?;
    Ok(stream)
}
