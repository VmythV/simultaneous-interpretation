//! 翻译：OpenAI 兼容 /chat/completions，流式输出

pub mod lang;
pub mod openai_chat;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// OpenAI 兼容 Base URL（含 /v1 或 /compatible-mode/v1）
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    /// 自定义提示词，可用占位符 {target} {context}；None 用默认
    pub prompt: Option<String>,
    /// 额外请求参数，原样合并进请求体（如 {"enable_thinking": false}）
    pub extra_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateSettings {
    pub llm: LlmConfig,
    /// 目标语言，如 "zh"
    pub target_lang: String,
    /// 原文已经是目标语言时，翻成这个语言，如 "en"
    pub fallback_lang: String,
    /// 带多少句上文
    pub context_size: usize,
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub text: String,
    pub target_lang: String,
    /// (原文, 译文)，按时间顺序
    pub context: Vec<(String, String)>,
}

pub const DEFAULT_PROMPT: &str = "你是专业的同声传译员。请把用户发来的一句话翻译成{target}。
要求：
- 只输出译文本身，不要解释，不要加引号，不要重复原文；
- 口语化、简洁、自然，符合{target}的表达习惯；
- 原文来自实时语音识别，可能有同音错字或断句不完整，请按最合理的意思翻译；
- 人名、专有名词、术语与上文保持一致。{context}";

pub fn build_messages(req: &TranslateRequest, template: Option<&str>) -> Vec<serde_json::Value> {
    let target = lang::display_name(&req.target_lang);
    let context = if req.context.is_empty() {
        String::new()
    } else {
        let mut s = String::from("\n\n上文（仅供理解，不要翻译）：");
        for (src, dst) in &req.context {
            s.push_str(&format!("\n原文：{src}\n译文：{dst}"));
        }
        s
    };
    let system = template.unwrap_or(DEFAULT_PROMPT).replace("{target}", &target).replace("{context}", &context);
    vec![
        serde_json::json!({ "role": "system", "content": system }),
        serde_json::json!({ "role": "user", "content": req.text }),
    ]
}
