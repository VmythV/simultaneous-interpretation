//! OpenAI 兼容的分段识别：POST {base_url}/audio/transcriptions

use anyhow::{Context, Result, bail};
use reqwest::multipart::{Form, Part};
use serde_json::Value;

use super::{SttConfig, clean_text};
use crate::wav;

pub async fn transcribe(client: &reqwest::Client, cfg: &SttConfig, samples: &[f32]) -> Result<String> {
    let audio = wav::encode(samples)?;
    let part = Part::bytes(audio).file_name("audio.wav").mime_str("audio/wav")?;
    let mut form = Form::new().part("file", part).text("model", cfg.model.clone()).text("response_format", "json");
    if let Some(lang) = cfg.language.as_deref().filter(|l| !l.is_empty() && *l != "auto") {
        form = form.text("language", lang.to_string());
    }
    let url = format!("{}/audio/transcriptions", cfg.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .multipart(form)
        .send()
        .await
        .with_context(|| format!("请求识别服务失败：{url}"))?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        bail!("识别服务返回 {status}：{}", truncate(&body, 300));
    }
    let v: Value = serde_json::from_str(&body).with_context(|| format!("识别结果不是 JSON：{}", truncate(&body, 200)))?;
    let text = v["text"].as_str().unwrap_or_default();
    Ok(clean_text(text))
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { s.chars().take(max).collect::<String>() + "…" }
}
