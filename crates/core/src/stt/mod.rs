//! 语音识别（STT）。两种模式：
//! - 流式（阿里云百炼实时识别）：边说边出字
//! - 分段（OpenAI 兼容 /audio/transcriptions）：VAD 断句后整句上传

pub mod aliyun;
pub mod openai;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SttProviderKind {
    /// 阿里云百炼实时语音识别（WebSocket 流式）
    AliyunRealtime,
    /// 任意 OpenAI 兼容的 /audio/transcriptions（分段）
    OpenaiCompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub provider: SttProviderKind,
    /// 流式：WebSocket 地址；分段：OpenAI 兼容 Base URL（含 /v1）
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 识别语言提示，如 "zh"、"en"；None 为自动
    pub language: Option<String>,
    /// 额外参数（流式：合并进 run-task 的 parameters，如 {"max_sentence_silence": 600}）
    #[serde(default)]
    pub extra_params: Option<serde_json::Value>,
}

/// 识别结果（内部通道）。`key` 标识一句话：同一个 key 的 Partial 会被 Final 覆盖。
#[derive(Debug, Clone)]
pub enum SttOut {
    Partial { key: u64, text: String },
    Final { key: u64, text: String },
    /// 流式模式下一个任务（一次 VAD 片段）已结束
    TaskDone { task: u64 },
    Error(String),
}

/// 清理识别结果：去掉 SenseVoice 一类模型可能带的 <|xx|> 标记、首尾空白
pub fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<|") {
        out.push_str(&rest[..start]);
        match rest[start..].find("|>") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Whisper 类模型在静音、噪音上常见的"幻听"句子。整句（去标点后）完全等于这些才丢弃。
const HALLUCINATIONS: &[&str] = &[
    "字幕由amara.org社区提供",
    "小编字幕由amara.org社区提供",
    "请不吝点赞订阅转发打赏支持明镜与点点栏目",
    "谢谢观看",
    "谢谢大家观看",
    "感谢观看",
    "中文字幕志愿者",
    "优优独播剧场yoyotelevisionseriesexclusive",
    "thankyouforwatching",
    "thanksforwatching",
    "pleasesubscribe",
    "you",
];

pub fn is_hallucination(text: &str) -> bool {
    let norm: String = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    norm.is_empty() || HALLUCINATIONS.contains(&norm.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_tags() {
        assert_eq!(clean_text("<|zh|><|NEUTRAL|><|Speech|>你好。"), "你好。");
        assert_eq!(clean_text("  hello "), "hello");
    }

    #[test]
    fn detects_hallucination() {
        assert!(is_hallucination("谢谢观看！"));
        assert!(is_hallucination("Thank you for watching."));
        assert!(is_hallucination("。"));
        assert!(!is_hallucination("谢谢观看这个演示的各位"));
    }
}
