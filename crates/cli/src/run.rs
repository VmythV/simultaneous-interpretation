//! 识别文件 / 翻译一句 / 完整流程

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use tingyi_core::audio::{CaptureMsg, Channel};
use tingyi_core::pipeline::{Pipeline, PipelineConfig, PipelineEvent};
use tingyi_core::stt::aliyun::AliyunStream;
use tingyi_core::stt::{SttOut, SttProviderKind, clean_text};
use tingyi_core::translate::{TranslateRequest, lang, openai_chat};
use tingyi_core::vad::VadConfig;
use tingyi_core::wav;
use tokio::sync::mpsc;

use crate::{Source, config, start_sources};

const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

pub async fn stt_file(file: &Path) -> Result<()> {
    let cfg = config::stt()?;
    let samples = wav::read(file)?;
    println!("识别 {}（{:.1} 秒），服务：{:?} / {}", file.display(), samples.len() as f32 / 16000.0, cfg.provider, cfg.model);
    let t0 = Instant::now();
    match cfg.provider {
        SttProviderKind::OpenaiCompatible => {
            let http = reqwest::Client::new();
            let text = tingyi_core::stt::openai::transcribe(&http, &cfg, &samples).await?;
            println!("{BOLD}{text}{RESET}  {DIM}({} ms){RESET}", t0.elapsed().as_millis());
        }
        SttProviderKind::AliyunRealtime => {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let stream = AliyunStream::spawn(cfg, tx);
            stream.begin();
            // 按接近实时的节奏发送（每 100ms 音频间隔 20ms），观察临时结果
            for chunk in samples.chunks(1600) {
                stream.audio(chunk.to_vec());
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let sent = Instant::now();
            stream.end();
            while let Some(out) = rx.recv().await {
                match out {
                    SttOut::Partial { text, .. } => println!("{DIM}… {text}{RESET}"),
                    SttOut::Final { text, .. } => {
                        println!("{BOLD}✔ {}{RESET}  {DIM}(发送完毕后 {} ms){RESET}", clean_text(&text), sent.elapsed().as_millis())
                    }
                    SttOut::TaskDone { .. } => break,
                    SttOut::Error(e) => bail!(e),
                }
            }
            println!("{DIM}总耗时 {} ms{RESET}", t0.elapsed().as_millis());
        }
    }
    Ok(())
}

pub async fn translate_once(text: &str) -> Result<()> {
    let settings = config::translate()?;
    let src = lang::detect(text);
    let target = lang::choose_target(src.as_deref(), &settings.target_lang, &settings.fallback_lang);
    println!("{DIM}原文语种 {:?} → 译为 {target}，模型 {}{RESET}", src, settings.llm.model);
    let http = reqwest::Client::new();
    let req = TranslateRequest { text: text.to_string(), target_lang: target, context: Vec::new() };
    let t0 = Instant::now();
    let mut first = None;
    print!("{BOLD}");
    let full = openai_chat::translate_stream(&http, &settings.llm, &req, |d| {
        first.get_or_insert_with(|| t0.elapsed());
        print!("{d}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    })
    .await?;
    println!("{RESET}");
    println!(
        "{DIM}首字 {} ms，完成 {} ms，{} 字{RESET}",
        first.unwrap_or_default().as_millis(),
        t0.elapsed().as_millis(),
        full.chars().count()
    );
    Ok(())
}

/// 按实时速度把音频送入流水线（每 100ms 一块），结尾补 1.5 秒静音让最后一句断句
fn play_file(samples: Vec<f32>, tx: tingyi_core::audio::CaptureSender) -> tokio::sync::oneshot::Receiver<()> {
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let tail = vec![0.0f32; 16000 * 3 / 2];
        for chunk in samples.chunks(1600).chain(tail.chunks(1600)) {
            tick.tick().await;
            if tx.send(CaptureMsg::Audio { channel: Channel::System, samples: chunk.to_vec() }).is_err() {
                break;
            }
        }
        let _ = done_tx.send(());
    });
    done_rx
}

#[derive(Default)]
struct Stats {
    stt: Vec<u64>,
    tr_first: Vec<u64>,
    tr_done: Vec<u64>,
    ttft: Vec<u64>,
}

fn summarize(name: &str, v: &mut [u64]) {
    if v.is_empty() {
        return;
    }
    v.sort_unstable();
    let avg = v.iter().sum::<u64>() / v.len() as u64;
    let p50 = v[v.len() / 2];
    let p90 = v[(v.len() * 9 / 10).min(v.len() - 1)];
    println!("  {name:<16} 平均 {avg:>5} ms   中位 {p50:>5} ms   P90 {p90:>5} ms   （{} 句）", v.len());
}

pub async fn run(
    source: Source,
    mic: Option<String>,
    secs: u64,
    no_translate: bool,
    file: Option<PathBuf>,
    silence_ms: u32,
) -> Result<()> {
    let stt = config::stt()?;
    let translate = if no_translate { None } else { Some(config::translate()?) };
    println!(
        "{DIM}识别：{:?} / {}；翻译：{}{RESET}",
        stt.provider,
        stt.model,
        translate.as_ref().map(|t| format!("{} → {}（原文是 {} 时译为 {}）", t.llm.model, t.target_lang, t.target_lang, t.fallback_lang)).unwrap_or("关闭".into())
    );
    let cfg = PipelineConfig { vad: VadConfig { min_silence_ms: silence_ms, ..Default::default() }, stt, translate };
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel();
    let pipeline = Pipeline::start(cfg, ev_tx)?;
    let mut captures = Vec::new();
    let mut file_done: Option<tokio::sync::oneshot::Receiver<()>> = None;
    if let Some(path) = &file {
        let samples = wav::read(path)?;
        println!("{BOLD}按实时速度回放 {}（{:.1} 秒）{RESET}", path.display(), samples.len() as f32 / 16000.0);
        file_done = Some(play_file(samples, pipeline.sender()));
    } else {
        captures = start_sources(source, mic.as_deref(), pipeline.sender())?;
        println!("{BOLD}开始监听，请说话（Ctrl+C 结束）{RESET}");
    }

    let deadline = (secs > 0).then(|| Instant::now() + Duration::from_secs(secs));
    let mut stats = Stats::default();
    let mut channel_of: HashMap<u64, Channel> = HashMap::new();
    let mut last_partial: HashMap<u64, String> = HashMap::new();
    let mut stopping = false;
    let mut shutdown = Some(pipeline);
    let mut shutdown_task: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        let sleep_until = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            _ = tokio::signal::ctrl_c(), if !stopping => { stopping = true; }
            _ = tokio::time::sleep_until(sleep_until.into()), if !stopping && deadline.is_some() => { stopping = true; }
            _ = async { file_done.as_mut().unwrap().await }, if !stopping && file_done.is_some() => { stopping = true; }
            ev = ev_rx.recv() => {
                let Some(ev) = ev else { break };
                print_event(&ev, &mut channel_of, &mut last_partial, &mut stats);
                continue;
            }
        }
        if stopping && shutdown_task.is_none() {
            println!("{DIM}正在停止，等待最后一句的结果……{RESET}");
            for c in &mut captures {
                c.stop();
            }
            let p = shutdown.take().unwrap();
            shutdown_task = Some(tokio::spawn(p.shutdown()));
        }
    }
    if let Some(t) = shutdown_task {
        let _ = t.await;
    }

    println!("\n{BOLD}延迟统计{RESET}（从说完一句开始计时）");
    summarize("出原文", &mut stats.stt);
    summarize("译文首字", &mut stats.tr_first);
    summarize("译文完成", &mut stats.tr_done);
    summarize("翻译请求首字", &mut stats.ttft);
    Ok(())
}

fn print_event(ev: &PipelineEvent, channel_of: &mut HashMap<u64, Channel>, last_partial: &mut HashMap<u64, String>, stats: &mut Stats) {
    let tag = |c: Channel| format!("[{}]", c.label());
    match ev {
        PipelineEvent::Info { message, .. } => println!("{DIM}ℹ {message}{RESET}"),
        PipelineEvent::Error { message, .. } => println!("{RED}✗ {message}{RESET}"),
        PipelineEvent::Level { .. } | PipelineEvent::SpeechStart { .. } | PipelineEvent::TrDelta { .. } | PipelineEvent::TrStart { .. } => {}
        PipelineEvent::SpeechEnd { channel, speech_ms } => println!("{DIM}{} ■ 断句（有效语音 {:.1}s）{RESET}", tag(*channel), *speech_ms as f32 / 1000.0),
        PipelineEvent::SttPartial { channel, utt_id, text } => {
            channel_of.insert(*utt_id, *channel);
            if last_partial.get(utt_id) != Some(text) {
                println!("{DIM}{} … {text}{RESET}", tag(*channel));
                last_partial.insert(*utt_id, text.clone());
            }
        }
        PipelineEvent::SttFinal { channel, utt_id, text, lang, latency_ms } => {
            channel_of.insert(*utt_id, *channel);
            last_partial.remove(utt_id);
            stats.stt.push(*latency_ms);
            println!("{BOLD}{} ✔ {text}{RESET}  {DIM}#{utt_id} {} 出原文 {latency_ms} ms{RESET}", tag(*channel), lang.as_deref().unwrap_or("?"));
        }
        PipelineEvent::SttDiscard { utt_id, .. } => {
            last_partial.remove(utt_id);
            println!("{DIM}#{utt_id} 已丢弃（空结果或幻听）{RESET}");
        }
        PipelineEvent::TrDone { utt_id, text, ttft_ms, speech_to_first_ms, speech_to_done_ms } => {
            stats.tr_first.push(*speech_to_first_ms);
            stats.tr_done.push(*speech_to_done_ms);
            stats.ttft.push(*ttft_ms);
            let indent = channel_of.get(utt_id).map(|c| " ".repeat(tag(*c).chars().count() + 2)).unwrap_or_default();
            println!(
                "{indent}{CYAN}→ {text}{RESET}  {DIM}#{utt_id} 译文首字 {speech_to_first_ms} ms / 完成 {speech_to_done_ms} ms（请求首字 {ttft_ms} ms）{RESET}"
            );
        }
        PipelineEvent::TrError { utt_id, message } => println!("{RED}#{utt_id} 翻译失败：{message}{RESET}"),
    }
}
