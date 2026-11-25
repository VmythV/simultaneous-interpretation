//! 用户设置：保存在 ~/Library/Application Support/app.tingyi.desktop/settings.json（权限 600，含 API Key）

use std::path::Path;

use serde::{Deserialize, Serialize};
use tingyi_core::pipeline::PipelineConfig;
use tingyi_core::stt::{SttConfig, SttProviderKind};
use tingyi_core::translate::{LlmConfig, TranslateSettings};
use tingyi_core::vad::VadConfig;

use crate::presets;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Mic,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 是否已完成首次启动向导
    pub onboarded: bool,
    pub source: Source,
    /// 麦克风设备 ID，空字符串为系统默认
    pub mic_device: String,
    pub target_lang: String,
    /// 原文已经是目标语言时，译成这个语言
    pub fallback_lang: String,
    pub auto_start: bool,
    /// 断句停顿时长（毫秒）
    pub silence_ms: u32,
    /// 单句最长时长（秒）
    pub max_segment_sec: u32,
    /// VAD 灵敏度阈值（越低越灵敏）
    pub vad_threshold: f32,
    pub stt: SttSettings,
    pub translate: LlmSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SttSettings {
    pub preset: String,
    pub mode: SttProviderKind,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    /// "auto" 或语言代码
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LlmSettings {
    /// 关闭后只转文字
    pub enabled: bool,
    pub preset: String,
    /// 与语音识别共用 Key（两边都是阿里云百炼时）
    pub use_stt_key: bool,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
    pub context_size: usize,
    /// 自定义提示词，空字符串用默认
    pub prompt: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            source: Source::Mic,
            mic_device: String::new(),
            target_lang: "zh".into(),
            fallback_lang: "en".into(),
            auto_start: false,
            silence_ms: 500,
            max_segment_sec: 15,
            vad_threshold: 0.5,
            stt: SttSettings::default(),
            translate: LlmSettings::default(),
        }
    }
}

impl Default for SttSettings {
    fn default() -> Self {
        let p = &presets::stt_presets()[0];
        Self {
            preset: p.id.into(),
            mode: p.mode,
            base_url: p.base_url.into(),
            model: p.model.into(),
            api_key: String::new(),
            language: "auto".into(),
        }
    }
}

impl Default for LlmSettings {
    fn default() -> Self {
        let p = &presets::llm_presets()[0];
        Self {
            enabled: true,
            preset: p.id.into(),
            use_stt_key: true,
            base_url: p.base_url.into(),
            model: p.model.into(),
            api_key: String::new(),
            temperature: 0.3,
            context_size: 3,
            prompt: String::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                // 文件损坏：备份后用默认值，避免丢失用户的 Key
                let backup = path.with_extension("json.bak");
                let _ = std::fs::copy(path, &backup);
                eprintln!("设置文件解析失败（已备份到 {}）：{e}", backup.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn stt_config(&self) -> Result<SttConfig, String> {
        let s = &self.stt;
        if s.api_key.trim().is_empty() {
            return Err("还没有填写语音识别的 API Key".into());
        }
        if s.base_url.trim().is_empty() || s.model.trim().is_empty() {
            return Err("语音识别的服务地址或模型为空".into());
        }
        Ok(SttConfig {
            provider: s.mode,
            base_url: s.base_url.trim().into(),
            api_key: s.api_key.trim().into(),
            model: s.model.trim().into(),
            language: Some(s.language.clone()).filter(|l| !l.is_empty() && l != "auto"),
            extra_params: None,
        })
    }

    /// 翻译实际使用的 API Key
    pub fn llm_key(&self) -> String {
        let t = &self.translate;
        if t.use_stt_key && t.preset == self.stt.preset { self.stt.api_key.clone() } else { t.api_key.clone() }
    }

    pub fn llm_config(&self) -> Result<LlmConfig, String> {
        let t = &self.translate;
        let key = self.llm_key();
        if key.trim().is_empty() {
            return Err("还没有填写翻译的 API Key".into());
        }
        if t.base_url.trim().is_empty() || t.model.trim().is_empty() {
            return Err("翻译的服务地址或模型为空".into());
        }
        Ok(LlmConfig {
            base_url: t.base_url.trim().into(),
            api_key: key.trim().into(),
            model: t.model.trim().into(),
            temperature: t.temperature,
            prompt: Some(t.prompt.clone()).filter(|p| !p.trim().is_empty()),
            extra_body: None,
        })
    }

    pub fn translate_settings(&self) -> Result<Option<TranslateSettings>, String> {
        if !self.translate.enabled {
            return Ok(None);
        }
        Ok(Some(TranslateSettings {
            llm: self.llm_config()?,
            target_lang: self.target_lang.clone(),
            fallback_lang: self.fallback_lang.clone(),
            context_size: self.translate.context_size,
        }))
    }

    pub fn pipeline_config(&self) -> Result<PipelineConfig, String> {
        Ok(PipelineConfig {
            vad: VadConfig {
                threshold: self.vad_threshold.clamp(0.2, 0.9),
                neg_threshold: (self.vad_threshold - 0.15).clamp(0.05, 0.8),
                min_silence_ms: self.silence_ms.clamp(200, 2000),
                max_segment_ms: self.max_segment_sec.clamp(5, 60) * 1000,
                ..Default::default()
            },
            stt: self.stt_config()?,
            translate: self.translate_settings()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_and_partial_json() {
        let s: Settings = serde_json::from_str(r#"{"targetLang":"ja","stt":{"apiKey":"k"}}"#).unwrap();
        assert_eq!(s.target_lang, "ja");
        assert_eq!(s.stt.api_key, "k");
        assert_eq!(s.stt.preset, "aliyun");
        assert_eq!(s.silence_ms, 500);
        // 共用 Key
        assert_eq!(s.llm_key(), "k");
        let cfg = s.pipeline_config().unwrap();
        assert_eq!(cfg.vad.min_silence_ms, 500);
        assert_eq!(cfg.translate.unwrap().llm.api_key, "k");
    }

    #[test]
    fn missing_key_is_reported() {
        let s = Settings::default();
        assert!(s.pipeline_config().unwrap_err().contains("API Key"));
    }
}
