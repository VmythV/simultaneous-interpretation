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
    /// 抢先翻译：流式识别时，VAD 一断句就用最后的临时结果开始翻译。
    /// 实测百炼在断句那一刻的临时结果常常还差最后一两个字，最终结果一变就得重译，
    /// 收益不稳定且多一次请求，所以默认关闭（见 docs/M2.md）。
    pub speculative: bool,
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

/// 交给翻译调度器的一句话
struct TrJob {
    utt_id: u64,
    channel: Channel,
    text: String,
    lang: Option<String>,
    /// 说完的时刻（计算延迟用）
    voice_end: Instant,
    kind: JobKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobKind {
    /// 抢先翻译：用断句时的临时结果
    Provisional,
    /// 最终结果。translated：抢先翻译的文本与最终结果一致，不用再翻译
    Final { translated: bool },
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
        // 预热翻译服务的连接（TLS 握手），第一句翻译不用再等建连接
        if let Some(t) = &shared.cfg.translate {
            let (http, url, key) = (shared.http.clone(), format!("{}/models", t.llm.base_url.trim_end_matches('/')), t.llm.api_key.clone());
            tokio::spawn(async move {
                let _ = http.get(url).bearer_auth(key).timeout(Duration::from_secs(5)).send().await;
            });
        }
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
    let (final_tx, final_rx) = mpsc::unbounded_channel::<TrJob>();
    // 不翻译时调度器也要运行：会议模式的回声去重在这里做
    let translator = tokio::spawn(translator(sh.cfg.translate.clone(), final_rx, sh.clone()));
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
    let _ = translator.await;
}

/// 一句话在识别阶段的记录
struct Pending {
    utt_id: u64,
    /// 断句时记录的"说完"时刻
    voice_end: Option<Instant>,
    /// 最近一次临时结果
    last_partial: String,
    /// 已经抢先翻译的文本（规范化后）
    speculated: Option<String>,
}

impl Pending {
    fn new(utt_id: u64, voice_end: Option<Instant>) -> Self {
        Self { utt_id, voice_end, last_partial: String::new(), speculated: None }
    }
}

async fn channel_worker(
    channel: Channel,
    mut audio: mpsc::UnboundedReceiver<Vec<f32>>,
    sh: Arc<Shared>,
    final_tx: mpsc::UnboundedSender<TrJob>,
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

    let speculative = sh.cfg.speculative && sh.cfg.translate.is_some() && stream.is_some();
    let mut pending: HashMap<u64, Pending> = HashMap::new();
    let mut seg_key: u64 = 0;
    // 流式：第几个识别任务（与 AliyunStream 内部的任务序号一致，用于从 key 找到所属任务）
    let mut task_no: u64 = 0;
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
                                task_no += 1;
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
                                if speculative {
                                    for (key, p) in pending.iter_mut() {
                                        if key / 10_000 != task_no || p.speculated.is_some() {
                                            continue;
                                        }
                                        let text = clean_text(&p.last_partial);
                                        if text.is_empty() || is_hallucination(&text) {
                                            continue;
                                        }
                                        p.speculated = Some(normalize(&text));
                                        p.voice_end = Some(voice_end);
                                        let _ = final_tx.send(TrJob {
                                            utt_id: p.utt_id,
                                            channel,
                                            lang: lang::detect(&text),
                                            text,
                                            voice_end,
                                            kind: JobKind::Provisional,
                                        });
                                    }
                                }
                            } else if speech_ms >= min_speech_ms {
                                seg_key += 1;
                                pending.insert(seg_key, Pending::new(sh.new_utt(), Some(voice_end)));
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
                    let p = pending.entry(key).or_insert_with(|| Pending::new(sh.new_utt(), None));
                    p.last_partial = text.clone();
                    sh.emit(PipelineEvent::SttPartial { channel, utt_id: p.utt_id, text });
                }
                SttOut::Final { key, text } => {
                    let p = pending.remove(&key).unwrap_or_else(|| Pending::new(sh.new_utt(), None));
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
                    let translated = p.speculated.as_deref() == Some(normalize(&text).as_str());
                    let _ = final_tx.send(TrJob { utt_id: p.utt_id, channel, text, lang, voice_end, kind: JobKind::Final { translated } });
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

/// 规范化文本：去掉标点和空白、转小写，用于比较"是不是同一句话"
pub fn normalize(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// 两句话是否基本相同（回声去重）：较短的一句被包含在较长的一句里（至少 6 个字且不少于一半长度），
/// 或者编辑距离相似度 ≥ 80%
pub fn is_echo(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    if short.is_empty() {
        return false;
    }
    if short.len() < 4 {
        return short == long;
    }
    if short.len() >= 6 && short.len() * 2 >= long.len() && long.windows(short.len()).any(|w| w == short.as_slice()) {
        return true;
    }
    let (n, m) = (short.len().min(200), long.len().min(200));
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = usize::from(short[i - 1] != long[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    1.0 - prev[m] as f32 / m.max(1) as f32 >= 0.8
}

/// 回声去重的时间窗口：两路的句子在这个时间内先后出现才比较
const ECHO_WINDOW: Duration = Duration::from_secs(4);

async fn translator(settings: Option<TranslateSettings>, mut rx: mpsc::UnboundedReceiver<TrJob>, sh: Arc<Shared>) {
    // 上文：(uttId, 原文, 译文)
    let context: Arc<Mutex<VecDeque<(u64, String, String)>>> = Arc::new(Mutex::new(VecDeque::new()));
    let settings = settings.map(Arc::new);
    let mut tasks = JoinSet::new();
    let mut inflight: HashMap<u64, tokio::task::AbortHandle> = HashMap::new();
    // 最近的最终结果：(uttId, 通道, 规范化文本, 时间)
    let mut recent: VecDeque<(u64, Channel, String, Instant)> = VecDeque::new();
    let mut discarded: std::collections::HashSet<u64> = Default::default();

    while let Some(job) = rx.recv().await {
        while tasks.try_join_next().is_some() {}
        if discarded.contains(&job.utt_id) {
            sh.emit(PipelineEvent::SttDiscard { channel: job.channel, utt_id: job.utt_id });
            continue;
        }

        if let JobKind::Final { translated } = job.kind {
            // 会议模式回声去重：麦克风录到了扬声器里对方的声音
            let norm = normalize(&job.text);
            recent.retain(|(_, _, _, at)| at.elapsed() < ECHO_WINDOW);
            let echo_of = recent.iter().position(|(_, ch, text, _)| *ch != job.channel && is_echo(text, &norm));
            match (job.channel, echo_of) {
                (Channel::Mic, Some(_)) => {
                    discard(&sh, &mut inflight, &mut discarded, Channel::Mic, job.utt_id);
                    continue;
                }
                (Channel::System, Some(i)) => {
                    let (mic_utt, ..) = recent.remove(i).unwrap();
                    discard(&sh, &mut inflight, &mut discarded, Channel::Mic, mic_utt);
                }
                _ => {}
            }
            recent.push_back((job.utt_id, job.channel, norm, Instant::now()));
            if translated {
                continue;
            }
        }

        let Some(settings) = settings.clone() else { continue };
        // 同一句的新翻译（最终结果与抢先翻译不一致）：取消旧的
        if let Some(old) = inflight.remove(&job.utt_id) {
            old.abort();
        }
        let target = {
            let (t, fb) = &*sh.langs.read().unwrap();
            lang::choose_target(job.lang.as_deref(), t, fb)
        };
        sh.emit(PipelineEvent::TrStart { utt_id: job.utt_id, target_lang: target.clone() });
        let ctx: Vec<(String, String)> =
            context.lock().unwrap().iter().filter(|(id, ..)| *id != job.utt_id).map(|(_, a, b)| (a.clone(), b.clone())).collect();
        let (sh, context) = (sh.clone(), context.clone());

        let handle = tasks.spawn(async move {
            let req = TranslateRequest { text: job.text.clone(), target_lang: target, context: ctx };
            let started = Instant::now();
            let mut first: Option<Instant> = None;
            for attempt in 1..=2 {
                let res = openai_chat::translate_stream(&sh.http, &settings.llm, &req, |delta| {
                    first.get_or_insert_with(Instant::now);
                    sh.emit(PipelineEvent::TrDelta { utt_id: job.utt_id, delta: delta.to_string() });
                })
                .await;
                match res {
                    Ok(text) => {
                        let first = first.unwrap_or_else(Instant::now);
                        sh.emit(PipelineEvent::TrDone {
                            utt_id: job.utt_id,
                            text: text.clone(),
                            ttft_ms: (first - started).as_millis() as u64,
                            speech_to_first_ms: first.saturating_duration_since(job.voice_end).as_millis() as u64,
                            speech_to_done_ms: job.voice_end.elapsed().as_millis() as u64,
                        });
                        if !text.is_empty() {
                            let mut c = context.lock().unwrap();
                            c.retain(|(id, ..)| *id != job.utt_id);
                            c.push_back((job.utt_id, job.text.clone(), text));
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
                        sh.emit(PipelineEvent::TrError { utt_id: job.utt_id, message: e.to_string() });
                        return;
                    }
                }
            }
        });
        inflight.insert(job.utt_id, handle);
        if inflight.len() > 64 {
            inflight.retain(|_, h| !h.is_finished());
        }
    }
    while tasks.join_next().await.is_some() {}
}

fn discard(
    sh: &Shared,
    inflight: &mut HashMap<u64, tokio::task::AbortHandle>,
    discarded: &mut std::collections::HashSet<u64>,
    channel: Channel,
    utt_id: u64,
) {
    if let Some(h) = inflight.remove(&utt_id) {
        h.abort();
    }
    discarded.insert(utt_id);
    sh.emit(PipelineEvent::SttDiscard { channel, utt_id });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_similarity() {
        assert!(is_echo(&normalize("我们下周发布第一个版本。"), &normalize("我们下周发布第一个版本")));
        assert!(is_echo(&normalize("下周发布第一个版本"), &normalize("我觉得我们下周发布第一个版本吧")));
        assert!(is_echo(&normalize("I think we should ship next week."), &normalize("I think we should ship it next week")));
        assert!(!is_echo(&normalize("好的"), &normalize("好的，那我们就按这个计划推进")));
        assert!(!is_echo(&normalize("今天天气很好"), &normalize("明天要开产品评审会")));
        assert!(!is_echo("", "abc"));
        // 对方一句短话不应吞掉我的长句
        assert!(!is_echo(&normalize("好的谢谢"), &normalize("好的谢谢大家，今天就先到这里吧")));
    }
}
