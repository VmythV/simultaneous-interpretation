//! 本地语种判断，用来决定翻译方向：
//! 原文不是目标语言 → 翻成目标语言；原文就是目标语言 → 翻成备用语言。

/// 判断文本语种，返回 ISO 639-1 代码（zh / en / ja / ko / ...）
pub fn detect(text: &str) -> Option<String> {
    let (mut han, mut kana, mut hangul, mut latin) = (0usize, 0usize, 0usize, 0usize);
    for c in text.chars() {
        match c as u32 {
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF => han += 1,
            0x3040..=0x30FF | 0x31F0..=0x31FF => kana += 1,
            0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => hangul += 1,
            _ if c.is_ascii_alphabetic() => latin += 1,
            _ => {}
        }
    }
    if kana >= 2 || (kana >= 1 && han + kana < 6) {
        return Some("ja".into());
    }
    if hangul > 0 && hangul >= han {
        return Some("ko".into());
    }
    // 中文里夹英文单词很常见（"这个 API 的 rate limit 太低了"），按字数加权
    if han > 0 && han * 3 >= latin {
        return Some("zh".into());
    }
    if latin == 0 {
        return None;
    }
    match whatlang::detect(text) {
        Some(info) if info.is_reliable() => Some(iso639_1(info.lang()).to_string()),
        // 短句不可靠时，拉丁字母默认按英文处理
        _ => Some("en".into()),
    }
}

fn iso639_1(lang: whatlang::Lang) -> &'static str {
    use whatlang::Lang::*;
    match lang {
        Eng => "en",
        Cmn => "zh",
        Jpn => "ja",
        Kor => "ko",
        Fra => "fr",
        Deu => "de",
        Spa => "es",
        Por => "pt",
        Ita => "it",
        Rus => "ru",
        Vie => "vi",
        Tha => "th",
        Ara => "ar",
        Nld => "nl",
        Pol => "pl",
        Tur => "tr",
        Ind => "id",
        Ukr => "uk",
        other => other.code(),
    }
}

/// 根据原文语种选择翻译目标
pub fn choose_target(source: Option<&str>, target: &str, fallback: &str) -> String {
    match source {
        Some(src) if base(src) == base(target) => fallback.to_string(),
        _ => target.to_string(),
    }
}

fn base(code: &str) -> &str {
    code.split(['-', '_']).next().unwrap_or(code)
}

/// 提示词里使用的语言名称
pub fn display_name(code: &str) -> String {
    match base(code) {
        "zh" => "简体中文",
        "en" => "英文",
        "ja" => "日文",
        "ko" => "韩文",
        "fr" => "法文",
        "de" => "德文",
        "es" => "西班牙文",
        "pt" => "葡萄牙文",
        "it" => "意大利文",
        "ru" => "俄文",
        "vi" => "越南文",
        "th" => "泰文",
        "ar" => "阿拉伯文",
        _ => return code.to_string(),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_cases() {
        assert_eq!(detect("今天天气很好，我们去公园散步吧").as_deref(), Some("zh"));
        assert_eq!(detect("这个 API 的 rate limit 太低了").as_deref(), Some("zh"));
        assert_eq!(detect("So the key idea of this paper is simple.").as_deref(), Some("en"));
        assert_eq!(detect("今日はいい天気ですね").as_deref(), Some("ja"));
        assert_eq!(detect("안녕하세요 반갑습니다").as_deref(), Some("ko"));
        assert_eq!(detect("OK").as_deref(), Some("en"));
        assert_eq!(detect("。。。"), None);
    }

    #[test]
    fn chooses_direction() {
        assert_eq!(choose_target(Some("zh"), "zh", "en"), "en");
        assert_eq!(choose_target(Some("en"), "zh", "en"), "zh");
        assert_eq!(choose_target(None, "zh", "en"), "zh");
        assert_eq!(choose_target(Some("zh"), "zh-CN", "en"), "en");
    }
}
