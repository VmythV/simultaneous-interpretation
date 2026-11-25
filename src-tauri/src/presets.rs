//! 服务商预设：选好服务商后只需填 API Key，地址和模型自动填好

use serde::Serialize;
use tingyi_core::stt::SttProviderKind;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SttPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub mode: SttProviderKind,
    pub base_url: &'static str,
    pub model: &'static str,
    /// 可选模型（下拉提示）
    pub models: Vec<&'static str>,
    pub key_url: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub base_url: &'static str,
    pub model: &'static str,
    pub models: Vec<&'static str>,
    pub key_url: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presets {
    pub stt: Vec<SttPreset>,
    pub llm: Vec<LlmPreset>,
    pub languages: Vec<Language>,
    /// 默认翻译提示词（设置页展示用）
    pub default_prompt: &'static str,
}

pub fn stt_presets() -> Vec<SttPreset> {
    vec![
        SttPreset {
            id: "aliyun",
            name: "阿里云百炼",
            description: "推荐 · 中文效果好 · 边说边出字",
            mode: SttProviderKind::AliyunRealtime,
            base_url: "wss://dashscope.aliyuncs.com/api-ws/v1/inference",
            model: "qwen-audio-3.1-asr-flash-streaming",
            models: vec!["qwen-audio-3.1-asr-flash-streaming", "fun-asr-realtime", "paraformer-realtime-v2"],
            key_url: "https://bailian.console.aliyun.com/?tab=model#/api-key",
            note: "同一个 Key 也能用于翻译。识别模型需要先在百炼控制台开通。",
        },
        SttPreset {
            id: "siliconflow",
            name: "硅基流动",
            description: "SenseVoice · 一句说完后出字",
            mode: SttProviderKind::OpenaiCompatible,
            base_url: "https://api.siliconflow.cn/v1",
            model: "FunAudioLLM/SenseVoiceSmall",
            models: vec!["FunAudioLLM/SenseVoiceSmall", "TeleAI/TeleSpeechASR"],
            key_url: "https://cloud.siliconflow.cn/account/ak",
            note: "",
        },
        SttPreset {
            id: "openai",
            name: "OpenAI",
            description: "Whisper / GPT-4o 转写 · 一句说完后出字",
            mode: SttProviderKind::OpenaiCompatible,
            base_url: "https://api.openai.com/v1",
            model: "gpt-4o-mini-transcribe",
            models: vec!["gpt-4o-mini-transcribe", "gpt-4o-transcribe", "whisper-1"],
            key_url: "https://platform.openai.com/api-keys",
            note: "",
        },
        SttPreset {
            id: "custom",
            name: "自定义",
            description: "任何兼容 OpenAI /audio/transcriptions 的服务",
            mode: SttProviderKind::OpenaiCompatible,
            base_url: "",
            model: "",
            models: vec![],
            key_url: "",
            note: "填写包含 /v1 的地址，例如 https://example.com/v1",
        },
    ]
}

pub fn llm_presets() -> Vec<LlmPreset> {
    vec![
        LlmPreset {
            id: "aliyun",
            name: "阿里云百炼",
            description: "推荐 · qwen-flash 快且便宜",
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            model: "qwen-flash",
            models: vec!["qwen-flash", "qwen-plus", "qwen-turbo"],
            key_url: "https://bailian.console.aliyun.com/?tab=model#/api-key",
            note: "",
        },
        LlmPreset {
            id: "deepseek",
            name: "DeepSeek",
            description: "deepseek-chat",
            base_url: "https://api.deepseek.com/v1",
            model: "deepseek-chat",
            models: vec!["deepseek-chat"],
            key_url: "https://platform.deepseek.com/api_keys",
            note: "",
        },
        LlmPreset {
            id: "openai",
            name: "OpenAI",
            description: "GPT 系列",
            base_url: "https://api.openai.com/v1",
            model: "gpt-4o-mini",
            models: vec!["gpt-4o-mini", "gpt-4.1-mini", "gpt-4.1"],
            key_url: "https://platform.openai.com/api-keys",
            note: "",
        },
        LlmPreset {
            id: "ollama",
            name: "Ollama（本地）",
            description: "本机运行，不需要联网",
            base_url: "http://localhost:11434/v1",
            model: "qwen2.5:7b",
            models: vec![],
            key_url: "",
            note: "Key 可随便填，例如 ollama",
        },
        LlmPreset {
            id: "custom",
            name: "自定义",
            description: "任何兼容 OpenAI /chat/completions 的服务",
            base_url: "",
            model: "",
            models: vec![],
            key_url: "",
            note: "填写包含 /v1 的地址，例如 https://example.com/v1",
        },
    ]
}

pub fn languages() -> Vec<Language> {
    [
        ("zh", "中文"),
        ("en", "英文"),
        ("ja", "日文"),
        ("ko", "韩文"),
        ("fr", "法文"),
        ("de", "德文"),
        ("es", "西班牙文"),
        ("ru", "俄文"),
        ("pt", "葡萄牙文"),
        ("it", "意大利文"),
        ("vi", "越南文"),
        ("th", "泰文"),
    ]
    .into_iter()
    .map(|(code, name)| Language { code, name })
    .collect()
}

pub fn all() -> Presets {
    Presets { stt: stt_presets(), llm: llm_presets(), languages: languages(), default_prompt: tingyi_core::translate::DEFAULT_PROMPT }
}
