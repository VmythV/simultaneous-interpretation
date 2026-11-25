//! 听译 M0 命令行 demo：验证音频采集、VAD 断句、语音识别和翻译。

mod config;
mod run;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use tingyi_core::audio::{self, CaptureHandle, CaptureMsg, Channel};
use tingyi_core::vad::{Segmenter, VadConfig, VadEvent};
use tingyi_core::wav;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "tingyi-cli", about = "听译 M0 技术验证工具")]
struct Cli {
    /// 读取 KEY=VALUE 配置的文件（API Key 等）
    #[arg(long, global = true, default_value = ".env")]
    env_file: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Source {
    Mic,
    System,
    Both,
}

#[derive(Subcommand)]
enum Cmd {
    /// 列出麦克风设备
    Devices,
    /// 检查 / 请求"系统录音"权限
    Perm {
        #[arg(long)]
        request: bool,
    },
    /// 采集音频并显示音量，可保存为 WAV
    Capture {
        #[arg(long, value_enum, default_value = "mic")]
        source: Source,
        #[arg(long)]
        mic: Option<String>,
        #[arg(long, default_value_t = 10)]
        secs: u64,
        /// 保存目录（每个通道一个 16kHz WAV）
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// 实时 VAD 断句
    Vad {
        #[arg(long, value_enum, default_value = "mic")]
        source: Source,
        #[arg(long)]
        mic: Option<String>,
        #[arg(long, default_value_t = 30)]
        secs: u64,
        /// 把每句保存为 WAV
        #[arg(long)]
        save: Option<PathBuf>,
        #[arg(long, default_value_t = 600)]
        silence_ms: u32,
    },
    /// 对 16kHz 单声道 WAV 文件做 VAD 断句
    VadFile {
        file: PathBuf,
        #[arg(long, default_value_t = 600)]
        silence_ms: u32,
    },
    /// 识别一个 16kHz 单声道 WAV 文件
    SttFile { file: PathBuf },
    /// 翻译一句话（自动判断方向）
    Translate { text: String },
    /// 完整流程：采集 → 断句 → 识别 → 翻译
    Run {
        #[arg(long, value_enum, default_value = "mic")]
        source: Source,
        #[arg(long)]
        mic: Option<String>,
        /// 运行时长（秒），0 表示一直运行到 Ctrl+C
        #[arg(long, default_value_t = 0)]
        secs: u64,
        /// 只识别不翻译
        #[arg(long)]
        no_translate: bool,
        /// 用 16kHz 单声道 WAV 文件按实时速度模拟"电脑声音"（不采集真实音频）
        #[arg(long)]
        file: Option<PathBuf>,
        /// 断句停顿时长（毫秒）
        #[arg(long, default_value_t = 500)]
        silence_ms: u32,
        /// 开启抢先翻译（对比延迟用）
        #[arg(long)]
        speculative: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    config::load_env_file(&cli.env_file);
    match cli.cmd {
        Cmd::Devices => devices(),
        Cmd::Perm { request } => perm(request),
        Cmd::Capture { source, mic, secs, out } => capture(source, mic, secs, out).await,
        Cmd::Vad { source, mic, secs, save, silence_ms } => vad_live(source, mic, secs, save, silence_ms).await,
        Cmd::VadFile { file, silence_ms } => vad_file(file, silence_ms),
        Cmd::SttFile { file } => run::stt_file(&file).await,
        Cmd::Translate { text } => run::translate_once(&text).await,
        Cmd::Run { source, mic, secs, no_translate, file, silence_ms, speculative } => {
            run::run(source, mic, secs, no_translate, file, silence_ms, speculative).await
        }
    }
}

fn devices() -> Result<()> {
    for d in audio::mic::list_input_devices()? {
        println!("{} {}  [{}]", if d.is_default { "*" } else { " " }, d.name, d.id);
    }
    Ok(())
}

fn perm(request: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let state = if request { audio::system_macos::request_permission()? } else { audio::system_macos::check_permission()? };
        println!("系统录音权限：{state}");
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        anyhow::bail!("仅 macOS 需要")
    }
}

pub fn start_sources(source: Source, mic: Option<&str>, tx: mpsc::UnboundedSender<CaptureMsg>) -> Result<Vec<CaptureHandle>> {
    let mut handles = Vec::new();
    if matches!(source, Source::Mic | Source::Both) {
        handles.push(audio::mic::start(mic, tx.clone())?);
    }
    if matches!(source, Source::System | Source::Both) {
        #[cfg(target_os = "macos")]
        handles.push(audio::system_macos::start(tx.clone())?);
        #[cfg(not(target_os = "macos"))]
        anyhow::bail!("电脑声音采集暂时只支持 macOS");
    }
    Ok(handles)
}

fn meter(rms: f32) -> String {
    // 对数刻度：-60dB ~ 0dB 映射到 20 格
    let db = 20.0 * rms.max(1e-6).log10();
    let n = (((db + 60.0) / 60.0) * 20.0).clamp(0.0, 20.0) as usize;
    format!("{}{} {:>6.1}dB", "█".repeat(n), "·".repeat(20 - n), db)
}

async fn capture(source: Source, mic: Option<String>, secs: u64, out: Option<PathBuf>) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let _handles = start_sources(source, mic.as_deref(), tx)?;
    let mut mic_buf = Vec::new();
    let mut sys_buf = Vec::new();
    let mut acc: [(f32, usize); 2] = [(0.0, 0), (0.0, 0)];
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline.into()) => break,
            _ = tokio::signal::ctrl_c() => break,
            _ = tick.tick() => {
                let line: Vec<String> = [(Channel::Mic, 0), (Channel::System, 1)]
                    .into_iter()
                    .filter(|(c, _)| match source { Source::Both => true, Source::Mic => *c == Channel::Mic, Source::System => *c == Channel::System })
                    .map(|(c, i)| {
                        let (sum, n) = acc[i];
                        let rms = if n > 0 { (sum / n as f32).sqrt() } else { 0.0 };
                        format!("{} {}", c.label(), meter(rms))
                    })
                    .collect();
                println!("{}", line.join("   "));
                acc = [(0.0, 0), (0.0, 0)];
            }
            msg = rx.recv() => match msg {
                Some(CaptureMsg::Audio { channel, samples }) => {
                    let i = if channel == Channel::Mic { 0 } else { 1 };
                    acc[i].0 += samples.iter().map(|s| s * s).sum::<f32>();
                    acc[i].1 += samples.len();
                    if out.is_some() {
                        if channel == Channel::Mic { mic_buf.extend(samples) } else { sys_buf.extend(samples) }
                    }
                }
                Some(CaptureMsg::Info { message, .. }) => println!("ℹ {message}"),
                Some(CaptureMsg::Error { message, .. }) => println!("✗ {message}"),
                None => break,
            }
        }
    }
    if let Some(dir) = out {
        std::fs::create_dir_all(&dir)?;
        for (name, buf) in [("mic.wav", &mic_buf), ("system.wav", &sys_buf)] {
            if !buf.is_empty() {
                let path = dir.join(name);
                wav::write(&path, buf)?;
                println!("已保存 {}（{:.1} 秒，整体 RMS {:.4}）", path.display(), buf.len() as f32 / 16000.0, audio::rms(buf));
            }
        }
    }
    Ok(())
}

fn print_vad_event(channel: Option<Channel>, ev: &VadEvent) {
    let tag = channel.map(|c| format!("[{}] ", c.label())).unwrap_or_default();
    match ev {
        VadEvent::SpeechStart { at_ms, .. } => println!("{tag}▶ 开始说话 @{:.2}s", *at_ms as f32 / 1000.0),
        VadEvent::SpeechEnd { at_ms, last_voice_ms, segment, speech_ms, forced } => println!(
            "{tag}■ 一句结束 @{:.2}s（最后人声 @{:.2}s，片段 {:.2}s，有效语音 {:.2}s{}）",
            *at_ms as f32 / 1000.0,
            *last_voice_ms as f32 / 1000.0,
            segment.len() as f32 / 16000.0,
            *speech_ms as f32 / 1000.0,
            if *forced { "，超长强制切断" } else { "" }
        ),
        VadEvent::Audio(_) => {}
    }
}

fn vad_file(file: PathBuf, silence_ms: u32) -> Result<()> {
    let samples = wav::read(&file)?;
    let mut seg = Segmenter::new(VadConfig { min_silence_ms: silence_ms, ..Default::default() })?;
    let t = Instant::now();
    let mut events = Vec::new();
    // 模拟实时：每次推 100ms
    for chunk in samples.chunks(1600) {
        events.extend(seg.push(chunk)?);
    }
    events.extend(seg.flush());
    let elapsed = t.elapsed();
    for ev in &events {
        print_vad_event(None, ev);
    }
    println!(
        "音频 {:.1}s，VAD 耗时 {:.1}ms（实时率 {:.4}）",
        samples.len() as f32 / 16000.0,
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() / (samples.len() as f64 / 16000.0)
    );
    Ok(())
}

async fn vad_live(source: Source, mic: Option<String>, secs: u64, save: Option<PathBuf>, silence_ms: u32) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let _handles = start_sources(source, mic.as_deref(), tx)?;
    let cfg = VadConfig { min_silence_ms: silence_ms, ..Default::default() };
    let mut segs = [Segmenter::new(cfg.clone())?, Segmenter::new(cfg)?];
    if let Some(dir) = &save {
        std::fs::create_dir_all(dir)?;
    }
    let mut count = 0;
    let deadline = Instant::now() + Duration::from_secs(secs);
    println!("开始监听 {secs} 秒，请说话……");
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline.into()) => break,
            _ = tokio::signal::ctrl_c() => break,
            msg = rx.recv() => match msg {
                Some(CaptureMsg::Audio { channel, samples }) => {
                    let seg = &mut segs[if channel == Channel::Mic { 0 } else { 1 }];
                    for ev in seg.push(&samples)? {
                        print_vad_event(Some(channel), &ev);
                        if let (VadEvent::SpeechEnd { segment, .. }, Some(dir)) = (&ev, &save) {
                            count += 1;
                            let path = dir.join(format!("{count:03}-{:?}.wav", channel).to_lowercase());
                            wav::write(&path, segment)?;
                            println!("    已保存 {}", path.display());
                        }
                    }
                }
                Some(CaptureMsg::Info { message, .. }) => println!("ℹ {message}"),
                Some(CaptureMsg::Error { message, .. }) => println!("✗ {message}"),
                None => break,
            }
        }
    }
    Ok(())
}
