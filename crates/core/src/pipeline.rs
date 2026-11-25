//! 流水线：音频 → VAD 断句 → 识别 → 翻译，统一以 [`PipelineEvent`] 输出给界面。
//!
//! 每个音源通道（我 / 对方）一条独立的 VAD + 识别；翻译共用一个调度器，按完成顺序维护上下文。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

use crate::audio::{CaptureMsg, CaptureSender, Channel};
use crate::stt::aliyun::AliyunStream;
use crate::stt::{SttConfig, SttOut, SttProviderKind, clean_text, is_hallucination};
use crate::translate::{TranslateRequest, TranslateSettings, lang, openai_chat};
use crate::vad::{Segmenter, VadConfig, VadEvent};

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub vad: VadConfig,
    pub stt: SttConfig,
    /// None 表示只转文字不翻译
    pub translate: Option<TranslateSettings>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PipelineEvent {
    Info { channel: Option<Channel>, message: String },
    Error { channel: Option<Channel>, message: String },
    /// 每 100ms 一次的音量
    Level { channel: Channel, rms: f32, speaking: bool },
    SpeechStart { channel: Channel },
    SpeechEnd { channel: Channel, speech_ms: u32 },
    /// 临时识别结果（同一 uttId 后续会被覆盖）
    SttPartial { channel: Channel, utt_id: u64, text: String },
    /// 确定的一句。latency_ms：说完到出结果
    SttFinal { channel: Channel, utt_id: u64, text: String, lang: Option<String>, latency_ms: u64 },
    /// 这一句识别为空或是"幻听"，界面应移除
    SttDiscard { channel: Channel, utt_id: u64 },
    TrStart { utt_id: u64, target_lang: String },
    TrDelta { utt_id: u64, delta: String },
    /// ttft_ms：请求到首字；speech_to_first_ms：说完到译文首字；speech_to_done_ms：说完到译文完成
    TrDone { utt_id: u64, text: String, ttft_ms: u64, speech_to_first_ms: u64, speech_to_done_ms: u64 },
    TrError { utt_id: u64, message: String },
}

pub type EventSender = mpsc::UnboundedSender<PipelineEvent>;

struct Shared {
    cfg: PipelineConfig,
    /// (目标语言, 备用语言)，运行中可修改
    langs: std::sync::RwLock<(String, String)>,
    events: EventSender,
    next_utt: AtomicU64,
    http: reqwest::Client,
}

impl Shared {
    fn emit(&self, ev: PipelineEvent) {
        let _ = self.events.send(ev);
    }
    fn new_utt(&self) -> u64 {
        self.next_utt.fetch_add(1, Ordering::SeqCst)
    }
}

struct FinalUtt {
    utt_id: u64,
    text: String,
    lang: Option<String>,
    voice_end: Instant,
}

pub struct Pipeline {
    shared: Arc<Shared>,
    input: CaptureSender,
    stop: Option<oneshot::Sender<()>>,
    router: JoinHandle<()>,
}

impl Pipeline {
    pub fn start(cfg: PipelineConfig, events: EventSender) -> Result<Self> {
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?;
        let langs = cfg
            .translate
            .as_ref()
            .map(|t| (t.target_lang.clone(), t.fallback_lang.clone()))
            .unwrap_or_else(|| ("zh".into(), "en".into()));
        let shared = Arc::new(Shared { cfg, langs: std::sync::RwLock::new(langs), events, next_utt: AtomicU64::new(1), http });
        let (input, input_rx) = mpsc::unbounded_channel();
        let (stop, stop_rx) = oneshot::channel();
        let router = tokio::spawn(router(shared.clone(), input_rx, stop_rx));
        Ok(Self { shared, input, stop: Some(stop), router })
    }

    /// 运行中修改翻译语言，对之后的句子生效
    pub fn set_languages(&self, target: &str, fallback: &str) {
        *self.shared.langs.write().unwrap() = (target.to_string(), fallback.to_string());
    }

    /// 音源采集用的发送端
    pub fn sender(&self) -> CaptureSender {
        self.input.clone()
    }

    /// 停止：结束正在说的句子，等待在途的识别和翻译完成（最多约 10 秒）
    pub async fn shutdown(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(12), &mut self.router).await;
    }
}

async fn router(sh: Arc<Shared>, mut input: mpsc::UnboundedReceiver<CaptureMsg>, mut stop: oneshot::Receiver<()>) {
    let (final_tx, final_rx) = mpsc::unbounded_channel::<FinalUtt>();
    let translator = sh.cfg.translate.clone().map(|t| tokio::spawn(translator(t, final_rx, sh.clone())));
    let mut workers: HashMap<Channel, (mpsc::UnboundedSender<Vec<f32>>, JoinHandle<()>)> = HashMap::new();

    loop {
        tokio::select! {
            _ = &mut stop => break,
            msg = input.recv() => match msg {
                None => break,
                Some(CaptureMsg::Audio { channel, samples }) => {
                    let (tx, _) = workers.entry(channel).or_insert_with(|| {
                        let (tx, rx) = mpsc::unbounded_channel();
                        (tx, tokio::spawn(channel_worker(channel, rx, sh.clone(), final_tx.clone())))
                    });
                    let _ = tx.send(samples);
                }
                Some(CaptureMsg::Info { channel, message }) => sh.emit(PipelineEvent::Info { channel: Some(channel), message }),
                Some(CaptureMsg::Error { channel, message }) => sh.emit(PipelineEvent::Error { channel: Some(channel), message }),
            }
        }
    }

    drop(final_tx);
    for (_, (tx, handle)) in workers {
        drop(tx);
        let _ = handle.await;
    }
    if let Some(t) = translator {
        let _ = t.await;
    }
}

/// 一句话在识别阶段的记录
struct Pending {
    utt_id: u64,
    /// 分段模式：断句时记录的"说完"时刻
    voice_end: Option<Instant>,
}

async fn channel_worker(
    channel: Channel,
    mut audio: mpsc::UnboundedReceiver<Vec<f32>>,
    sh: Arc<Shared>,
    final_tx: mpsc::UnboundedSender<FinalUtt>,
) {
    let mut seg = match Segmenter::new(sh.cfg.vad.clone()) {
        Ok(s) => s,
        Err(e) => {
            sh.emit(PipelineEvent::Error { channel: Some(channel), message: format!("VAD 初始化失败：{e}") });
            return;
        }
    };
    let threshold = sh.cfg.vad.threshold;
    let min_speech_ms = sh.cfg.vad.min_speech_ms;
    let (stt_tx, mut stt_rx) = mpsc::unbounded_channel::<SttOut>();
    let stream = (sh.cfg.stt.provider == SttProviderKind::AliyunRealtime)
        .then(|| AliyunStream::spawn(sh.cfg.stt.clone(), stt_tx.clone()));

    let mut pending: HashMap<u64, Pending> = HashMap::new();
    let mut seg_key: u64 = 0;
    let mut open_work: usize = 0; // 流式：未结束的任务数；分段：在途的识别请求数
    let mut last_voice = Instant::now();
    let (mut lvl_sum, mut lvl_n, mut lvl_at) = (0f32, 0usize, Instant::now());
    let mut audio_closed = false;
    let mut drain_deadline: Option<Instant> = None;

    loop {
        if audio_closed && open_work == 0 {
            break;
        }
        let deadline = drain_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            samples = audio.recv(), if !audio_closed => {
                let vad_events = match samples {
                    Some(samples) => {
                        lvl_sum += samples.iter().map(|s| s * s).sum::<f32>();
                        lvl_n += samples.len();
                        let evs = match seg.push(&samples) {
                            Ok(evs) => evs,
                            Err(e) => {
                                sh.emit(PipelineEvent::Error { channel: Some(channel), message: format!("VAD 出错：{e}") });
                                Vec::new()
                            }
                        };
                        if seg.is_speaking() && seg.last_prob() >= threshold {
                            last_voice = Instant::now();
                        }
                        if lvl_at.elapsed() >= Duration::from_millis(100) && lvl_n > 0 {
                            let rms = (lvl_sum / lvl_n as f32).sqrt();
                            sh.emit(PipelineEvent::Level { channel, rms, speaking: seg.is_speaking() });
                            (lvl_sum, lvl_n, lvl_at) = (0.0, 0, Instant::now());
                        }
                        evs
                    }
                    None => {
                        audio_closed = true;
                        drain_deadline = Some(Instant::now() + Duration::from_secs(10));
                        seg.flush().into_iter().collect()
                    }
                };
                for ev in vad_events {
                    match ev {
                        VadEvent::SpeechStart { pre_roll, .. } => {
                            sh.emit(PipelineEvent::SpeechStart { channel });
                            if let Some(s) = &stream {
                                s.begin();
                                s.audio(pre_roll);
                                open_work += 1;
                            }
                        }
                        VadEvent::Audio(samples) => {
                            if let Some(s) = &stream {
                                s.audio(samples);
                            }
                        }
                        VadEvent::SpeechEnd { at_ms, last_voice_ms, segment, speech_ms, .. } => {
                            sh.emit(PipelineEvent::SpeechEnd { channel, speech_ms });
                            let voice_end = Instant::now() - Duration::from_millis(at_ms.saturating_sub(last_voice_ms));
                            if let Some(s) = &stream {
                                s.end();
                            } else if speech_ms >= min_speech_ms {
                                seg_key += 1;
                                pending.insert(seg_key, Pending { utt_id: sh.new_utt(), voice_end: Some(voice_end) });
                                open_work += 1;
                                let (cfg, http, tx, key) = (sh.cfg.stt.clone(), sh.http.clone(), stt_tx.clone(), seg_key);
                                tokio::spawn(async move {
                                    let out = match crate::stt::openai::transcribe(&http, &cfg, &segment).await {
                                        Ok(text) => SttOut::Final { key, text },
                                        Err(e) => SttOut::Error(format!("#{key} {e}")),
                                    };
                                    let _ = tx.send(out);
                                });
                            }
                        }
                    }
                }
            }
            Some(out) = stt_rx.recv() => match out {
                SttOut::Partial { key, text } => {
                    let utt_id = pending.entry(key).or_insert_with(|| Pending { utt_id: sh.new_utt(), voice_end: None }).utt_id;
                    sh.emit(PipelineEvent::SttPartial { channel, utt_id, text });
                }
                SttOut::Final { key, text } => {
                    let p = pending.remove(&key).unwrap_or_else(|| Pending { utt_id: sh.new_utt(), voice_end: None });
                    if stream.is_none() {
                        open_work = open_work.saturating_sub(1);
                    }
                    let text = clean_text(&text);
                    if text.is_empty() || is_hallucination(&text) {
                        sh.emit(PipelineEvent::SttDiscard { channel, utt_id: p.utt_id });
                        continue;
                    }
                    let voice_end = p.voice_end.unwrap_or(last_voice);
                    let lang = lang::detect(&text);
                    sh.emit(PipelineEvent::SttFinal {
                        channel,
                        utt_id: p.utt_id,
                        text: text.clone(),
                        lang: lang.clone(),
                        latency_ms: voice_end.elapsed().as_millis() as u64,
                    });
                    let _ = final_tx.send(FinalUtt { utt_id: p.utt_id, text, lang, voice_end });
                }
                SttOut::TaskDone { task } => {
                    open_work = open_work.saturating_sub(1);
                    // 有临时结果却没有最终结果的句子：移除
                    let stale: Vec<u64> = pending.keys().copied().filter(|k| k / 10_000 == task).collect();
                    for k in stale {
                        if let Some(p) = pending.remove(&k) {
                            sh.emit(PipelineEvent::SttDiscard { channel, utt_id: p.utt_id });
                        }
                    }
                }
                SttOut::Error(message) => {
                    open_work = open_work.saturating_sub(1);
                    sh.emit(PipelineEvent::Error { channel: Some(channel), message });
                }
            },
            _ = tokio::time::sleep_until(deadline.into()) => {
                sh.emit(PipelineEvent::Info { channel: Some(channel), message: "等待识别结果超时，已放弃剩余的句子".into() });
                break;
            }
        }
    }
}

async fn translator(settings: TranslateSettings, mut rx: mpsc::UnboundedReceiver<FinalUtt>, sh: Arc<Shared>) {
    let context: Arc<Mutex<VecDeque<(String, String)>>> = Arc::new(Mutex::new(VecDeque::new()));
    let settings = Arc::new(settings);
    let mut tasks = JoinSet::new();

    while let Some(f) = rx.recv().await {
        let target = {
            let (t, fb) = &*sh.langs.read().unwrap();
            lang::choose_target(f.lang.as_deref(), t, fb)
        };
        sh.emit(PipelineEvent::TrStart { utt_id: f.utt_id, target_lang: target.clone() });
        let ctx: Vec<(String, String)> = context.lock().unwrap().iter().cloned().collect();
        let (sh, settings, context) = (sh.clone(), settings.clone(), context.clone());

        tasks.spawn(async move {
            let req = TranslateRequest { text: f.text.clone(), target_lang: target, context: ctx };
            let started = Instant::now();
            let mut first: Option<Instant> = None;
            for attempt in 1..=2 {
                let res = openai_chat::translate_stream(&sh.http, &settings.llm, &req, |delta| {
                    first.get_or_insert_with(Instant::now);
                    sh.emit(PipelineEvent::TrDelta { utt_id: f.utt_id, delta: delta.to_string() });
                })
                .await;
                match res {
                    Ok(text) => {
                        let first = first.unwrap_or_else(Instant::now);
                        sh.emit(PipelineEvent::TrDone {
                            utt_id: f.utt_id,
                            text: text.clone(),
                            ttft_ms: (first - started).as_millis() as u64,
                            speech_to_first_ms: (first - f.voice_end).as_millis() as u64,
                            speech_to_done_ms: f.voice_end.elapsed().as_millis() as u64,
                        });
                        if !text.is_empty() {
                            let mut c = context.lock().unwrap();
                            c.push_back((f.text.clone(), text));
                            while c.len() > settings.context_size {
                                c.pop_front();
                            }
                        }
                        return;
                    }
                    // 还没输出任何内容时可以安全重试一次
                    Err(_) if attempt == 1 && first.is_none() => {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                    Err(e) => {
                        sh.emit(PipelineEvent::TrError { utt_id: f.utt_id, message: e.to_string() });
                        return;
                    }
                }
            }
        });
        while tasks.try_join_next().is_some() {}
    }
    while tasks.join_next().await.is_some() {}
}
