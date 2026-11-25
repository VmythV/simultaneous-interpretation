//! Silero VAD（v6，ONNX）+ 断句状态机
//!
//! 模型每次吃 512 个 16kHz 采样（32ms），前面拼上上一块的最后 64 个采样作为上下文，
//! 输入：input[1, 576] f32、state[2, 1, 128] f32、sr 标量 i64；输出：概率[1, 1]、新 state。

use std::collections::VecDeque;
use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use ort::session::Session;
use ort::value::Tensor;

use crate::audio::TARGET_RATE;

const MODEL: &[u8] = include_bytes!("../../../resources/silero_vad.onnx");
pub const WINDOW: usize = 512;
const CONTEXT: usize = 64;
const WINDOW_MS: u32 = (WINDOW as u32 * 1000) / TARGET_RATE; // 32ms

fn env() -> Result<&'static ort::environment::Environment> {
    static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();
    if let Some(e) = ENV.get() {
        return Ok(e);
    }
    let e = ort::init().with_name("tingyi").build().map_err(|e| anyhow!("初始化 ONNX Runtime 失败：{e}"))?;
    Ok(ENV.get_or_init(|| e))
}

pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
}

impl SileroVad {
    pub fn new() -> Result<Self> {
        let session = Session::builder(env()?)
            .map_err(|e| anyhow!("{e}"))?
            .with_intra_threads(1)
            .map_err(|e| anyhow!("{e}"))?
            .with_inter_threads(1)
            .map_err(|e| anyhow!("{e}"))?
            .commit_from_memory(MODEL)
            .map_err(|e| anyhow!("加载 VAD 模型失败：{e}"))?;
        Ok(Self { session, state: vec![0.0; 2 * 128], context: vec![0.0; CONTEXT] })
    }

    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
    }

    /// 输入恰好 512 个采样，返回"有人说话"的概率 0~1
    pub fn prob(&mut self, window: &[f32]) -> Result<f32> {
        debug_assert_eq!(window.len(), WINDOW);
        let mut input = Vec::with_capacity(CONTEXT + WINDOW);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(window);
        self.context.copy_from_slice(&input[input.len() - CONTEXT..]);

        let input = Tensor::from_array(([1usize, CONTEXT + WINDOW], input))?;
        let state = Tensor::from_array(([2usize, 1, 128], self.state.clone()))?;
        let sr = Tensor::from_array(((), vec![TARGET_RATE as i64]))?;
        let outputs = self.session.run(ort::inputs![input, state, sr])?;
        let (_, prob) = outputs[0].try_extract_tensor::<f32>()?;
        let (_, new_state) = outputs[1].try_extract_tensor::<f32>()?;
        let p = prob[0];
        self.state.copy_from_slice(new_state);
        Ok(p)
    }
}

#[derive(Debug, Clone)]
pub struct VadConfig {
    /// 概率高于它算"开始说话"
    pub threshold: f32,
    /// 概率低于它算"静音"（滞回，避免抖动）
    pub neg_threshold: f32,
    /// 连续静音多久算一句结束
    pub min_silence_ms: u32,
    /// 开始说话前保留多少音频（避免吞掉第一个字）
    pub pre_roll_ms: u32,
    /// 单句最长时长，超过强制切断
    pub max_segment_ms: u32,
    /// 有效语音短于它的片段，标记为可丢弃（咳嗽、敲桌子等）
    pub min_speech_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            neg_threshold: 0.35,
            min_silence_ms: 600,
            pre_roll_ms: 300,
            max_segment_ms: 15_000,
            min_speech_ms: 250,
        }
    }
}

#[derive(Debug)]
pub enum VadEvent {
    /// 开始说话。`pre_roll` 是开始前缓存的音频，应先于后续 Audio 发送给识别
    SpeechStart { at_ms: u64, pre_roll: Vec<f32> },
    /// 说话中的音频（含句中短暂停顿）
    Audio(Vec<f32>),
    /// 一句结束
    SpeechEnd {
        at_ms: u64,
        /// 最后一次检测到人声的时间（真正"说完"的时刻）
        last_voice_ms: u64,
        /// 这一句完整音频（含 pre-roll）
        segment: Vec<f32>,
        /// 有效语音时长
        speech_ms: u32,
        /// 是否因超长被强制切断
        forced: bool,
    },
}

pub struct Segmenter {
    vad: SileroVad,
    cfg: VadConfig,
    pending: Vec<f32>,
    pre_roll: VecDeque<f32>,
    in_speech: bool,
    segment: Vec<f32>,
    silence_ms: u32,
    speech_ms: u32,
    processed: u64,
    last_voice_ms: u64,
    last_prob: f32,
}

impl Segmenter {
    pub fn new(cfg: VadConfig) -> Result<Self> {
        Ok(Self {
            vad: SileroVad::new()?,
            cfg,
            pending: Vec::new(),
            pre_roll: VecDeque::new(),
            in_speech: false,
            segment: Vec::new(),
            silence_ms: 0,
            speech_ms: 0,
            processed: 0,
            last_voice_ms: 0,
            last_prob: 0.0,
        })
    }

    pub fn is_speaking(&self) -> bool {
        self.in_speech
    }

    pub fn last_prob(&self) -> f32 {
        self.last_prob
    }

    /// 当前已处理的音频时长（毫秒）
    pub fn now_ms(&self) -> u64 {
        self.processed * 1000 / TARGET_RATE as u64
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<VadEvent>> {
        self.pending.extend_from_slice(samples);
        let mut events = Vec::new();
        let mut offset = 0;
        while self.pending.len() - offset >= WINDOW {
            let window: Vec<f32> = self.pending[offset..offset + WINDOW].to_vec();
            offset += WINDOW;
            self.process_window(window, &mut events)?;
        }
        self.pending.drain(..offset);
        Ok(events)
    }

    /// 立即结束当前句子（停止采集时调用）
    pub fn flush(&mut self) -> Option<VadEvent> {
        if self.in_speech { Some(self.end_segment(false)) } else { None }
    }

    fn process_window(&mut self, window: Vec<f32>, events: &mut Vec<VadEvent>) -> Result<()> {
        let p = self.vad.prob(&window)?;
        self.last_prob = p;
        self.processed += WINDOW as u64;
        let now = self.now_ms();

        if !self.in_speech {
            if p >= self.cfg.threshold {
                self.in_speech = true;
                self.silence_ms = 0;
                self.speech_ms = WINDOW_MS;
                self.last_voice_ms = now;
                let pre_roll: Vec<f32> = self.pre_roll.drain(..).collect();
                self.segment = pre_roll.clone();
                self.segment.extend_from_slice(&window);
                let start = now.saturating_sub(((pre_roll.len() + WINDOW) as u64) * 1000 / TARGET_RATE as u64);
                events.push(VadEvent::SpeechStart { at_ms: start, pre_roll });
                events.push(VadEvent::Audio(window));
            } else {
                self.pre_roll.extend(window);
                let max = (self.cfg.pre_roll_ms * TARGET_RATE / 1000) as usize;
                while self.pre_roll.len() > max {
                    self.pre_roll.pop_front();
                }
            }
            return Ok(());
        }

        // 说话中
        self.segment.extend_from_slice(&window);
        events.push(VadEvent::Audio(window));
        if p < self.cfg.neg_threshold {
            self.silence_ms += WINDOW_MS;
        } else {
            self.silence_ms = 0;
            if p >= self.cfg.threshold {
                self.speech_ms += WINDOW_MS;
                self.last_voice_ms = now;
            }
        }

        let seg_ms = (self.segment.len() as u64 * 1000 / TARGET_RATE as u64) as u32;
        if self.silence_ms >= self.cfg.min_silence_ms {
            events.push(self.end_segment(false));
        } else if seg_ms >= self.cfg.max_segment_ms {
            events.push(self.end_segment(true));
            // 强制切断后如果还在说话，立刻开始下一句
            self.in_speech = true;
            self.silence_ms = 0;
            self.speech_ms = 0;
            events.push(VadEvent::SpeechStart { at_ms: now, pre_roll: Vec::new() });
        }
        Ok(())
    }

    fn end_segment(&mut self, forced: bool) -> VadEvent {
        self.in_speech = false;
        let segment = std::mem::take(&mut self.segment);
        let ev = VadEvent::SpeechEnd {
            at_ms: self.now_ms(),
            last_voice_ms: self.last_voice_ms,
            segment,
            speech_ms: self.speech_ms,
            forced,
        };
        self.silence_ms = 0;
        self.speech_ms = 0;
        ev
    }

    pub fn config(&self) -> &VadConfig {
        &self.cfg
    }

    pub fn reset(&mut self) {
        self.vad.reset();
        self.pending.clear();
        self.pre_roll.clear();
        self.segment.clear();
        self.in_speech = false;
    }
}
