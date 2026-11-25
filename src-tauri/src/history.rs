//! 历史记录：每次"开始 → 停止"保存为一个 JSON 文件，另有 index.json 存摘要；以及导出 TXT / Markdown / SRT。
//!
//! 目录：~/Library/Application Support/app.tingyi.desktop/history/

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tingyi_core::audio::Channel;
use tingyi_core::pipeline::PipelineEvent;

use crate::settings::Source;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub channel: Channel,
    pub text: String,
    #[serde(default)]
    pub translation: String,
    /// 相对开始时间的毫秒数（SRT 用）
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    /// Unix 毫秒
    pub started_at: u64,
    pub ended_at: u64,
    pub source: Source,
    pub target_lang: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub started_at: u64,
    pub ended_at: u64,
    pub source: Source,
    pub count: usize,
    pub preview: String,
}

impl Record {
    fn summary(&self) -> Summary {
        let first = self.entries.first();
        let preview = first.map(|e| if e.translation.is_empty() { e.text.clone() } else { format!("{} {}", e.text, e.translation) });
        Summary {
            id: self.id.clone(),
            started_at: self.started_at,
            ended_at: self.ended_at,
            source: self.source,
            count: self.entries.len(),
            preview: preview.unwrap_or_default().chars().take(80).collect(),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

struct Current {
    record: Record,
    started: Instant,
    /// uttId → 第一次出现（临时结果）的时间
    first_seen: HashMap<u64, u64>,
    save: bool,
    dirty: bool,
}

pub struct Recorder {
    dir: PathBuf,
    current: Mutex<Option<Current>>,
    index_lock: Mutex<()>,
}

impl Recorder {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, current: Mutex::new(None), index_lock: Mutex::new(()) }
    }

    /// 开始新的一次记录（上一次如有未写入的内容先写盘）
    pub fn begin(&self, source: Source, target_lang: &str, save: bool) {
        self.flush();
        let started_at = now_ms();
        *self.current.lock().unwrap() = Some(Current {
            record: Record {
                id: started_at.to_string(),
                started_at,
                ended_at: started_at,
                source,
                target_lang: target_lang.into(),
                entries: Vec::new(),
            },
            started: Instant::now(),
            first_seen: HashMap::new(),
            save,
            dirty: false,
        });
    }

    pub fn end(&self) {
        if let Some(c) = self.current.lock().unwrap().as_mut() {
            c.record.ended_at = now_ms();
            c.dirty = true;
        }
        self.flush();
    }

    pub fn on_event(&self, ev: &PipelineEvent) {
        let mut guard = self.current.lock().unwrap();
        let Some(c) = guard.as_mut() else { return };
        let now = c.started.elapsed().as_millis() as u64;
        match ev {
            PipelineEvent::SttPartial { utt_id, .. } => {
                c.first_seen.entry(*utt_id).or_insert(now);
            }
            PipelineEvent::SttFinal { channel, utt_id, text, latency_ms, .. } => {
                let start = c.first_seen.get(utt_id).copied().unwrap_or(now.saturating_sub(2000));
                let end = now.saturating_sub(*latency_ms).max(start);
                match c.record.entries.iter_mut().find(|e| e.id == *utt_id) {
                    Some(e) => e.text = text.clone(),
                    None => c.record.entries.push(Entry {
                        id: *utt_id,
                        channel: *channel,
                        text: text.clone(),
                        translation: String::new(),
                        start_ms: start,
                        end_ms: end,
                    }),
                }
                c.record.entries.sort_by_key(|e| e.id);
                c.dirty = true;
            }
            PipelineEvent::SttDiscard { utt_id, .. } => {
                c.record.entries.retain(|e| e.id != *utt_id);
                c.dirty = true;
            }
            PipelineEvent::TrDone { utt_id, text, .. } => {
                if let Some(e) = c.record.entries.iter_mut().find(|e| e.id == *utt_id) {
                    e.translation = text.clone();
                    c.dirty = true;
                }
            }
            _ => {}
        }
    }

    /// 有改动时写盘（定时调用）
    pub fn flush(&self) {
        let record = {
            let mut guard = self.current.lock().unwrap();
            let Some(c) = guard.as_mut() else { return };
            if !c.dirty || !c.save || c.record.entries.is_empty() {
                return;
            }
            c.dirty = false;
            c.record.clone()
        };
        if let Err(e) = self.write(&record) {
            eprintln!("保存历史记录失败：{e}");
        }
    }

    fn write(&self, record: &Record) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{}.json", record.id));
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(record)?)?;
        std::fs::rename(&tmp, &path)?;
        let _g = self.index_lock.lock().unwrap();
        let mut index = self.read_index();
        index.retain(|s| s.id != record.id);
        index.push(record.summary());
        self.write_index(&mut index)
    }

    fn read_index(&self) -> Vec<Summary> {
        std::fs::read(self.dir.join("index.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write_index(&self, index: &mut Vec<Summary>) -> anyhow::Result<()> {
        index.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.dir.join("index.json"), serde_json::to_vec(index)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Summary> {
        let _g = self.index_lock.lock().unwrap();
        let mut index = self.read_index();
        index.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        index
    }

    fn safe_path(&self, id: &str) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(!id.is_empty() && id.chars().all(|c| c.is_ascii_digit()), "无效的记录编号");
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn get(&self, id: &str) -> anyhow::Result<Record> {
        let bytes = std::fs::read(self.safe_path(id)?).map_err(|_| anyhow::anyhow!("找不到这条记录"))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let path = self.safe_path(id)?;
        let _ = std::fs::remove_file(path);
        let _g = self.index_lock.lock().unwrap();
        let mut index = self.read_index();
        index.retain(|s| s.id != id);
        self.write_index(&mut index)
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        let _g = self.index_lock.lock().unwrap();
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        Ok(())
    }
}

// ───────────── 导出 ─────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Txt,
    Md,
    Srt,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Txt => "txt",
            Format::Md => "md",
            Format::Srt => "srt",
        }
    }
    pub fn filter_name(self) -> &'static str {
        match self {
            Format::Txt => "纯文本",
            Format::Md => "Markdown",
            Format::Srt => "SRT 字幕",
        }
    }
}

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

fn srt_time(ms: u64) -> String {
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// 渲染导出内容。title 由前端按本地时间格式化好传进来。
pub fn render(title: &str, entries: &[Entry], format: Format) -> String {
    let meeting = entries.iter().any(|e| e.channel == Channel::Mic) && entries.iter().any(|e| e.channel == Channel::System);
    let who = |e: &Entry| if meeting { format!("{}：", e.channel.label()) } else { String::new() };
    let mut out = String::new();
    match format {
        Format::Txt => {
            out.push_str(&format!("{title}\n\n"));
            for e in entries {
                out.push_str(&format!("[{}] {}{}\n", clock(e.start_ms), who(e), e.text));
                if !e.translation.is_empty() {
                    out.push_str(&format!("{}\n", e.translation));
                }
                out.push('\n');
            }
        }
        Format::Md => {
            out.push_str(&format!("# {title}\n\n"));
            for e in entries {
                let speaker = if meeting { format!(" · {}", e.channel.label()) } else { String::new() };
                out.push_str(&format!("**{}{}**　{}\n", clock(e.start_ms), speaker, e.text));
                if !e.translation.is_empty() {
                    out.push_str(&format!("\n> {}\n", e.translation));
                }
                out.push('\n');
            }
        }
        Format::Srt => {
            for (i, e) in entries.iter().enumerate() {
                let end = if e.end_ms > e.start_ms { e.end_ms } else { e.start_ms + 2000 };
                out.push_str(&format!("{}\n{} --> {}\n", i + 1, srt_time(e.start_ms), srt_time(end)));
                if !e.translation.is_empty() {
                    out.push_str(&format!("{}\n", e.translation));
                }
                out.push_str(&format!("{}{}\n\n", who(e), e.text));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry { id: 1, channel: Channel::System, text: "Hello everyone.".into(), translation: "大家好。".into(), start_ms: 1200, end_ms: 2600 },
            Entry { id: 2, channel: Channel::Mic, text: "你好。".into(), translation: "Hi.".into(), start_ms: 65_000, end_ms: 65_000 },
        ]
    }

    #[test]
    fn renders_formats() {
        let txt = render("听译记录", &entries(), Format::Txt);
        assert!(txt.contains("[00:01] 对方：Hello everyone.\n大家好。"));
        let md = render("听译记录", &entries(), Format::Md);
        assert!(md.starts_with("# 听译记录"));
        assert!(md.contains("**01:05 · 我**　你好。"));
        let srt = render("x", &entries(), Format::Srt);
        assert!(srt.starts_with("1\n00:00:01,200 --> 00:00:02,600\n大家好。\n对方：Hello everyone.\n\n2\n00:01:05,000 --> 00:01:07,000"));
    }

    #[test]
    fn recorder_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tingyi-history-test-{}", now_ms()));
        let rec = Recorder::new(dir.clone());
        rec.begin(Source::Mic, "zh", true);
        rec.on_event(&PipelineEvent::SttPartial { channel: Channel::Mic, utt_id: 1, text: "你".into() });
        rec.on_event(&PipelineEvent::SttFinal { channel: Channel::Mic, utt_id: 1, text: "你好".into(), lang: None, latency_ms: 0 });
        rec.on_event(&PipelineEvent::TrDone { utt_id: 1, text: "Hello".into(), ttft_ms: 0, speech_to_first_ms: 0, speech_to_done_ms: 0 });
        rec.end();
        let list = rec.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].count, 1);
        let r = rec.get(&list[0].id).unwrap();
        assert_eq!(r.entries[0].translation, "Hello");
        rec.delete(&list[0].id).unwrap();
        assert!(rec.list().is_empty());
        assert!(rec.get("../etc").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
