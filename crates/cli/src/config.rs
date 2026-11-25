//! M0 配置：从 .env 文件和环境变量读取服务地址与 API Key。
//!
//! 常用变量（.env.example 里有完整说明）：
//!   DASHSCOPE_API_KEY      阿里云百炼 Key（识别 + 翻译默认都用它）
//!   TINGYI_STT             aliyun（默认，流式）| openai（分段）
//!   TINGYI_STT_BASE_URL / TINGYI_STT_KEY / TINGYI_STT_MODEL / TINGYI_STT_LANGUAGE / TINGYI_STT_PARAMS
//!   TINGYI_LLM_BASE_URL / TINGYI_LLM_KEY / TINGYI_LLM_MODEL
//!   TINGYI_TARGET（默认 zh） / TINGYI_FALLBACK（默认 en）

use std::path::Path;

use anyhow::{Result, bail};
use tingyi_core::stt::{SttConfig, SttProviderKind};
use tingyi_core::translate::{LlmConfig, TranslateSettings};

pub fn load_env_file(path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().trim_start_matches("export ").trim();
            let v = v.trim().trim_matches('"').trim_matches('\'');
            // 已有的环境变量优先
            if std::env::var_os(k).is_none() && !v.is_empty() {
                // SAFETY: 在启动阶段、其他线程创建之前调用
                unsafe { std::env::set_var(k, v) };
            }
        }
    }
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub const ALIYUN_WS_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference";
pub const ALIYUN_COMPAT_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";

pub fn stt() -> Result<SttConfig> {
    let provider = match var("TINGYI_STT").as_deref() {
        None | Some("aliyun") => SttProviderKind::AliyunRealtime,
        Some("openai") => SttProviderKind::OpenaiCompatible,
        Some(other) => bail!("TINGYI_STT 只能是 aliyun 或 openai，当前是 {other}"),
    };
    let (default_url, default_model, key_var) = match provider {
        SttProviderKind::AliyunRealtime => (ALIYUN_WS_URL, "qwen-audio-3.1-asr-flash-streaming", "DASHSCOPE_API_KEY"),
        SttProviderKind::OpenaiCompatible => ("https://api.siliconflow.cn/v1", "FunAudioLLM/SenseVoiceSmall", "SILICONFLOW_API_KEY"),
    };
    let api_key = var("TINGYI_STT_KEY").or_else(|| var(key_var));
    let Some(api_key) = api_key else {
        bail!("缺少识别服务 API Key：请在 .env 里设置 {key_var} 或 TINGYI_STT_KEY");
    };
    Ok(SttConfig {
        provider,
        base_url: var("TINGYI_STT_BASE_URL").unwrap_or_else(|| default_url.into()),
        api_key,
        model: var("TINGYI_STT_MODEL").unwrap_or_else(|| default_model.into()),
        language: var("TINGYI_STT_LANGUAGE"),
        extra_params: var("TINGYI_STT_PARAMS").map(|s| serde_json::from_str(&s)).transpose()?,
    })
}

pub fn translate() -> Result<TranslateSettings> {
    let Some(api_key) = var("TINGYI_LLM_KEY").or_else(|| var("DASHSCOPE_API_KEY")) else {
        bail!("缺少翻译服务 API Key：请在 .env 里设置 DASHSCOPE_API_KEY 或 TINGYI_LLM_KEY");
    };
    let extra_body = var("TINGYI_LLM_EXTRA").map(|s| serde_json::from_str(&s)).transpose()?;
    Ok(TranslateSettings {
        llm: LlmConfig {
            base_url: var("TINGYI_LLM_BASE_URL").unwrap_or_else(|| ALIYUN_COMPAT_URL.into()),
            api_key,
            model: var("TINGYI_LLM_MODEL").unwrap_or_else(|| "qwen-flash".into()),
            temperature: var("TINGYI_LLM_TEMPERATURE").and_then(|t| t.parse().ok()).unwrap_or(0.3),
            prompt: None,
            extra_body,
        },
        target_lang: var("TINGYI_TARGET").unwrap_or_else(|| "zh".into()),
        fallback_lang: var("TINGYI_FALLBACK").unwrap_or_else(|| "en".into()),
        context_size: 3,
    })
}
