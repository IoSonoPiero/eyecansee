//! Translations (Fluent, `locales/*.ftl`) and manual (`docs/manual.*.md`), embedded in the executable.
//! The online help shows the same file as the manual, so they cannot drift apart.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// To add a language: translate `locales/en.ftl` and `docs/manual.en.md`, then add a line.
/// The first line is the fallback language for missing messages.
const LANGS: &[(&str, &str, &str)] = &[
    ("en", include_str!("../locales/en.ftl"), include_str!("../docs/manual.en.md")),
    ("it", include_str!("../locales/it.ftl"), include_str!("../docs/manual.it.md")),
];

pub struct I18n {
    bundle: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
    pub manual: &'static str,
}

fn bundle(code: &str, ftl: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = code.parse().expect("valid language code");
    let mut b = FluentBundle::new(vec![lang]);
    // No Unicode isolation characters around arguments: egui would show them.
    b.set_use_isolating(false);
    b.add_resource(FluentResource::try_new(ftl.to_string()).expect("valid .ftl file"))
        .expect("no duplicate messages");
    b
}

/// Available languages: code and name in the language itself (`language-name`), for the Options.
pub fn languages() -> Vec<(&'static str, String)> {
    LANGS
        .iter()
        .map(|&(code, ftl, _)| {
            let b = bundle(code, ftl);
            let name = b.get_message("language-name").and_then(|m| m.value()).map(|p| b.format_pattern(p, None, &mut vec![]).into_owned());
            (code, name.unwrap_or_else(|| code.to_string()))
        })
        .collect()
}

impl I18n {
    /// `EYECANSEE_LANG` if set, otherwise the one chosen in the Options (`chosen`), otherwise
    /// the system language, otherwise English.
    pub fn detect(chosen: Option<&str>) -> Self {
        let wanted = std::env::var("EYECANSEE_LANG")
            .ok()
            .or_else(|| chosen.map(str::to_string))
            .or_else(sys_locale::get_locale)
            .unwrap_or_default();
        Self::new(&wanted)
    }

    pub fn new(wanted: &str) -> Self {
        // "it-IT", "it_IT.UTF-8", "it" → "it"
        let prefix = wanted.split(['-', '_', '.']).next().unwrap_or("").to_lowercase();
        let &(code, ftl, manual) = LANGS.iter().find(|l| l.0 == prefix).unwrap_or(&LANGS[0]);
        Self { bundle: bundle(code, ftl), fallback: bundle(LANGS[0].0, LANGS[0].1), manual }
    }

    pub fn t(&self, id: &str) -> String {
        self.t_args(id, &[])
    }

    pub fn t_args(&self, id: &str, args: &[(&str, &str)]) -> String {
        let mut fargs = FluentArgs::new();
        for &(k, v) in args {
            // Whole numbers as numbers, so the messages can choose singular or plural
            // (`{ $n -> [one] … *[other] … }`); "007" or "1e5" (names) stay text.
            let number = !v.is_empty() && v.len() < 16 && v.bytes().all(|b| b.is_ascii_digit()) && (v == "0" || !v.starts_with('0'));
            fargs.set(k, if number { FluentValue::from(v.parse::<u64>().unwrap_or(0)) } else { FluentValue::from(v) });
        }
        for b in [&self.bundle, &self.fallback] {
            if let Some(pattern) = b.get_message(id).and_then(|m| m.value()) {
                return b.format_pattern(pattern, Some(&fargs), &mut vec![]).into_owned();
            }
        }
        id.to_string() // message missing everywhere: the id shows, so the bug is obvious
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(ftl: &str) -> Vec<String> {
        let mut ids: Vec<String> = ftl
            .lines()
            .filter_map(|l| l.split_once(" =").map(|(id, _)| id.trim().to_string()))
            .filter(|id| !id.is_empty() && !id.starts_with('#'))
            .collect();
        ids.sort();
        ids
    }

    /// Whole numbers choose singular or plural; a name made of digits with a leading zero stays as it is.
    #[test]
    fn plural() {
        let it = I18n::new("it");
        assert!(it.t_args("batch-skipped", &[("n", "1")]).starts_with("**1 immagine** viene esclusa"));
        assert!(it.t_args("batch-skipped", &[("n", "3")]).starts_with("**3 immagini** vengono escluse"));
        let en = I18n::new("en");
        assert!(en.t_args("batch-turn-confirm", &[("op", "X"), ("n", "1")]).contains("**1 image** will be"));
        assert!(en.t_args("save-format", &[("name", "007")]).contains("**007**"));
    }

    fn headings(md: &str) -> usize {
        md.lines().filter(|l| l.starts_with('#')).count()
    }

    /// Every language has the same messages as English and a manual with the same structure.
    #[test]
    fn languages_in_sync() {
        let (en, en_ftl, en_manual) = LANGS[0];
        for &(code, ftl, manual) in LANGS {
            bundle(code, ftl);
            assert_eq!(ids(ftl), ids(en_ftl), "messages of {code} differ from {en}");
            assert_eq!(headings(manual), headings(en_manual), "manual {code} has different sections from {en}");
        }
        assert_eq!(I18n::new("it_IT.UTF-8").t_args("counter", &[("n", "1"), ("total", "9")]), "1 di 9");
        assert_eq!(I18n::new("xx").t("menu-help"), "Help");
        assert_eq!(languages(), vec![("en", "English".to_string()), ("it", "Italiano".to_string())]);
    }
}
