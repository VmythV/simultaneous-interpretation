//! OpenAI 兼容 /chat/completions 流式翻译（SSE）

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use serde_json::{Value, json};

use super::{LlmConfig, TranslateRequest, build_messages};
use crate::stt::openai::truncate;

/// 流式翻译：每收到一段增量就调用 `on_delta`，返回完整译文
pub async fn translate_stream(
    client: &reqwest::Client,
    cfg: &LlmConfig,
    req: &TranslateRequest,
    mut on_delta: impl FnMut(&str),
) -> Result<String> {
    let mut body = json!({
        "model": cfg.model,
        "stream": true,
        "temperature": cfg.temperature,
        "messages": build_messages(req, cfg.prompt.as_deref()),
    });
    if let (Some(Value::Object(extra)), Value::Object(map)) = (&cfg.extra_body, &mut body) {
        for (k, v) in extra {
            map.insert(k.clone(), v.clone());
        }
    }

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(&cfg.api_key)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("请求翻译服务失败：{url}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("翻译服务返回 {status}：{}", truncate(&text, 300));
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut full = String::new();
    'outer: while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk.context("读取翻译结果失败")?);
        // 按行切分；只解码完整的行，避免把多字节汉字切断
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                break 'outer;
            }
            let v: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(err) = v.get("error") {
                bail!("翻译服务报错：{err}");
            }
            if let Some(delta) = v["choices"][0]["delta"]["content"].as_str()
                && !delta.is_empty()
            {
                full.push_str(delta);
                on_delta(delta);
            }
        }
    }
    Ok(full.trim().to_string())
}
