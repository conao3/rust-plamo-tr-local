use anyhow::{Result, bail};
use whatlang::Lang;

pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
    detected_as: Option<Lang>,
}

const fn language(code: &'static str, name: &'static str, detected_as: Option<Lang>) -> Language {
    Language {
        code,
        name,
        detected_as,
    }
}

pub const LANGUAGES: &[Language] = &[
    language("ja", "Japanese", Some(Lang::Jpn)),
    language("en", "English", Some(Lang::Eng)),
    language("zh-Hans", "Chinese (Simplified)", Some(Lang::Cmn)),
    language("zh-Hant", "Chinese (Traditional)", None),
    language("ko", "Korean", Some(Lang::Kor)),
    language("az", "Azerbaijani", Some(Lang::Aze)),
    language("ar", "Arabic", Some(Lang::Ara)),
    language("hy", "Armenian", Some(Lang::Hye)),
    language("it", "Italian", Some(Lang::Ita)),
    language("id", "Indonesian", Some(Lang::Ind)),
    language("uz", "Uzbek", Some(Lang::Uzb)),
    language("ur", "Urdu", Some(Lang::Urd)),
    language("et", "Estonian", Some(Lang::Est)),
    language("nl", "Dutch", Some(Lang::Nld)),
    language("kk", "Kazakh", None),
    language("ca", "Catalan", Some(Lang::Cat)),
    language("gu", "Gujarati", Some(Lang::Guj)),
    language("km", "Khmer", Some(Lang::Khm)),
    language("hr", "Croatian", Some(Lang::Hrv)),
    language("jv", "Javanese", Some(Lang::Jav)),
    language("ka", "Georgian", Some(Lang::Kat)),
    language("si", "Sinhala", Some(Lang::Sin)),
    language("es", "Spanish", Some(Lang::Spa)),
    language("sk", "Slovak", Some(Lang::Slk)),
    language("sl", "Slovenian", Some(Lang::Slv)),
    language("sw", "Swahili", None),
    language("ceb", "Cebuano", None),
    language("sr", "Serbian", Some(Lang::Srp)),
    language("th", "Thai", Some(Lang::Tha)),
    language("tl", "Tagalog", Some(Lang::Tgl)),
    language("ta", "Tamil", Some(Lang::Tam)),
    language("te", "Telugu", Some(Lang::Tel)),
    language("de", "German", Some(Lang::Deu)),
    language("ne", "Nepali", Some(Lang::Nep)),
    language("eu", "Basque", None),
    language("hu", "Hungarian", Some(Lang::Hun)),
    language("pa", "Punjabi", Some(Lang::Pan)),
    language("my", "Burmese", Some(Lang::Mya)),
    language("hi", "Hindi", Some(Lang::Hin)),
    language("fr", "French", Some(Lang::Fra)),
    language("bg", "Bulgarian", Some(Lang::Bul)),
    language("vi", "Vietnamese", Some(Lang::Vie)),
    language("he", "Hebrew", Some(Lang::Heb)),
    language("be", "Belarusian", Some(Lang::Bel)),
    language("fa", "Persian", Some(Lang::Pes)),
    language("bn", "Bengali", Some(Lang::Ben)),
    language("pl", "Polish", Some(Lang::Pol)),
    language("mr", "Marathi", Some(Lang::Mar)),
    language("ml", "Malayalam", Some(Lang::Mal)),
    language("ms", "Malay", None),
    language("lv", "Latvian", Some(Lang::Lav)),
    language("lt", "Lithuanian", Some(Lang::Lit)),
    language("ru", "Russian", Some(Lang::Rus)),
];

pub fn parse_language(value: &str) -> Result<&'static str, String> {
    LANGUAGES
        .iter()
        .find(|language| language.code.eq_ignore_ascii_case(value))
        .map(|language| language.code)
        .ok_or_else(|| {
            format!("unsupported language `{value}` (see `plamo-tr-local language list-languages`)")
        })
}

pub fn detect(text: &str) -> Option<&'static str> {
    let lang = whatlang::detect_lang(text)?;
    LANGUAGES
        .iter()
        .find(|language| language.detected_as == Some(lang))
        .map(|language| language.code)
}

fn is_pivot(code: &str) -> bool {
    code == "ja" || code == "en"
}

#[derive(Debug, PartialEq, Eq)]
pub struct Direction {
    pub source: Option<&'static str>,
    pub target: &'static str,
}

pub fn resolve(
    source: Option<&'static str>,
    target: Option<&'static str>,
    text: &str,
) -> Result<Direction> {
    let detected = match source {
        Some(_) => None,
        None => detect(text),
    };
    let target = target.unwrap_or(match source.or(detected) {
        Some("ja") => "en",
        _ => "ja",
    });
    let source = match source {
        Some(source) => Some(source),
        None if is_pivot(target) => None,
        None => match detected {
            Some(detected) => Some(detected),
            None => bail!(
                "could not detect the source language; pass --source-language-code to translate into {target}"
            ),
        },
    };
    if let Some(source) = source {
        if source == target {
            bail!("source and target are both {source}");
        }
        if !is_pivot(source) && !is_pivot(target) {
            bail!(
                "{source} -> {target} is not supported: either side must be Japanese (ja) or English (en)"
            );
        }
    }
    Ok(Direction { source, target })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direction(source: Option<&'static str>, target: &'static str) -> Direction {
        Direction { source, target }
    }

    #[test]
    fn detects_japanese_and_english() {
        assert_eq!(detect("今日はいい天気ですね。"), Some("ja"));
        assert_eq!(
            detect("The weather is nice today, so let us go for a walk."),
            Some("en")
        );
    }

    #[test]
    fn japanese_defaults_to_english() {
        let resolved = resolve(None, None, "今日はいい天気ですね。").unwrap();
        assert_eq!(resolved, direction(None, "en"));
    }

    #[test]
    fn other_languages_default_to_japanese() {
        let resolved = resolve(None, None, "Bonjour tout le monde, comment allez-vous ?").unwrap();
        assert_eq!(resolved, direction(None, "ja"));
    }

    #[test]
    fn given_source_decides_target() {
        assert_eq!(
            resolve(Some("ja"), None, "").unwrap(),
            direction(Some("ja"), "en")
        );
        assert_eq!(
            resolve(Some("ko"), None, "").unwrap(),
            direction(Some("ko"), "ja")
        );
    }

    #[test]
    fn non_pivot_target_sends_detected_source() {
        let resolved = resolve(
            None,
            Some("ko"),
            "The weather is nice today, so let us go for a walk.",
        )
        .unwrap();
        assert_eq!(resolved, direction(Some("en"), "ko"));
    }

    #[test]
    fn rejects_pairs_without_japanese_or_english() {
        assert!(resolve(Some("ko"), Some("fr"), "").is_err());
    }

    #[test]
    fn rejects_same_language() {
        assert!(resolve(Some("en"), Some("en"), "").is_err());
    }

    #[test]
    fn parses_codes_case_insensitively() {
        assert_eq!(parse_language("zh-hans"), Ok("zh-Hans"));
        assert!(parse_language("xx").is_err());
    }
}
