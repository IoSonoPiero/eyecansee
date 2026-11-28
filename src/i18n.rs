//! Translations (Fluent, `locales/*.ftl`), embedded in the executable. The guide's pages are in
//! `help.rs`.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// To add a language: translate `locales/en.ftl` and the guide's pages (`docs/help/en/`, listed in
/// `help.rs`), then add a line. The first line is the fallback language for missing messages.
const LANGS: &[(&str, &str)] = &[("en", include_str!("../locales/en.ftl")), ("it", include_str!("../locales/it.ftl"))];

pub struct I18n {
    bundle: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
    /// The language chosen (`en`, `it`), for the guide's pages.
    pub code: &'static str,
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
        .map(|&(code, ftl)| {
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
        let &(code, ftl) = LANGS.iter().find(|l| l.0 == prefix).unwrap_or(&LANGS[0]);
        Self { bundle: bundle(code, ftl), fallback: bundle(LANGS[0].0, LANGS[0].1), code }
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

    /// `t_args` for a place that shows the text as it is (the error bar, the status bar, a
    /// label, the log): without the message's own `**`, which only a message window shows as bold
    /// (Piero, 28/11), and with the values as they are, a `*` in a name too. The values pass
    /// protected through `save_dialogs::plain`, which takes the `**` away; only what `plain` reads
    /// is protected, so a value that chooses a variant ("ply-binary") stays as it is.
    pub fn t_plain(&self, id: &str, args: &[(&str, &str)]) -> String {
        let kept: Vec<(&str, String)> = args.iter().map(|&(k, v)| (k, v.chars().flat_map(|c| matches!(c, '\\' | '*' | '`').then_some('\\').into_iter().chain([c])).collect())).collect();
        let kept: Vec<(&str, &str)> = kept.iter().map(|(k, v)| (*k, v.as_str())).collect();
        crate::save_dialogs::plain(&self.t_args(id, &kept))
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
        assert_eq!(en.t_args("transfer-links-heading", &[("n", "1")]), "1 link to a folder skipped:");
        assert_eq!(en.t_args("transfer-links-heading", &[("n", "3")]), "3 links to folders skipped:");
        assert_eq!(it.t_args("transfer-links-heading", &[("n", "1")]), "1 collegamento a una cartella saltato:");
        assert_eq!(it.t_args("transfer-links-heading", &[("n", "3")]), "3 collegamenti a cartelle saltati:");
    }

    /// Every error id written in the code (`"error-…"`) has its message.
    #[test]
    fn error_ids_exist() {
        let known = ids(LANGS[0].1);
        let mut paths = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
        let mut missing = vec![];
        while let Some(p) = paths.pop() {
            if p.is_dir() {
                paths.extend(std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().path()));
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for rest in text.split("\"error-").skip(1) {
                let tail = rest.split('"').next().unwrap();
                let id = format!("error-{tail}");
                let plain = !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
                if plain && !known.contains(&id) {
                    missing.push(format!("{id} ({})", p.display()));
                }
            }
        }
        assert!(missing.is_empty(), "error ids without a message: {missing:?}");
        assert!(I18n::new("it").error(crate::source::SILENT, "nas").starts_with("Il server nas non risponde"));
    }

    /// Every language has the same messages as English.
    #[test]
    fn languages_in_sync() {
        let (en, en_ftl) = LANGS[0];
        for &(code, ftl) in LANGS {
            bundle(code, ftl);
            assert_eq!(ids(ftl), ids(en_ftl), "messages of {code} differ from {en}");
        }
        assert_eq!(I18n::new("it_IT.UTF-8").t_args("counter", &[("n", "1"), ("total", "9")]), "1 di 9");
        assert_eq!(I18n::new("xx").t("menu-help"), "Help");
        assert_eq!(languages(), vec![("en", "English".to_string()), ("it", "Italiano".to_string())]);
    }

    /// Messages go to a new line between sentences: the same lines in English and Italian, none
    /// that starts or ends with a space (a line of the .ftl file indented differently).
    #[test]
    fn lines_alike() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let mut different = Vec::new();
        for id in ids(LANGS[0].1) {
            // Each variant of the messages that choose one.
            for (n, known, how, what) in [("2", "yes", "trash", "images"), ("1", "no", "permanent", "videos"), ("0", "no", "trash", "mixed")] {
                let args = [("n", n), ("detail", "D"), ("name", "N"), ("known", known), ("how", how), ("what", what)];
                let (a, b) = (en.t_args(&id, &args), it.t_args(&id, &args));
                for line in a.lines().chain(b.lines()) {
                    assert_eq!(line, line.trim(), "{id}: a line with spaces around it");
                }
                if a.lines().count() != b.lines().count() {
                    different.push(format!("{id} {n} {known} {how} {what}"));
                }
            }
        }
        assert!(different.is_empty(), "not the same lines in English and Italian: {different:?}");
    }

    /// Piero, 28/11: in a message window the name, the folder, the address the message is about
    /// are in bold, kept as they are inside (`md`: a `*` and a `_`); where the same message is shown
    /// as it is (the status bar, the copy box), `t_plain` gives it without `**` and with the value
    /// as it is.
    #[test]
    fn names_in_bold_in_the_windows() {
        let v = "Al*bum_1/x.jpg";
        // The message, its other values, and the one in bold.
        type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);
        let cases: [Case; 11] = [
            ("transfer-to", &[], "dir"),
            ("transfer-files-copied", &[("n", "2")], "dir"),
            ("status-file-removed", &[], "name"),
            ("bm-missing-place", &[("name", "Foto")], "address"),
            ("bm-missing-phone", &[("name", "Foto")], "address"),
            ("phone-stalled-upload", &[], "name"),
            ("delete-phone-busy", &[("what", "copy"), ("s", "3"), ("button", "Delete")], "name"),
            ("status-phone-deaf", &[], "name"),
            ("quit-phone-ask", &[("what", "move")], "name"),
            ("batch-images", &[("n", "3")], "folder"),
            ("summary-saved", &[("n", "3")], "folder"),
        ];
        for lang in ["en", "it"] {
            let i18n = I18n::new(lang);
            for (id, args, key) in cases {
                let kept = crate::save_dialogs::md(v);
                let window = i18n.t_args(id, &[args, &[(key, kept.as_str())]].concat());
                assert!(window.contains(&format!("**{kept}**")), "{lang} {id}: {v} not in bold: {window}");
                let plain = i18n.t_plain(id, &[args, &[(key, v)]].concat());
                assert!(!plain.contains("**") && plain.contains(v), "{lang} {id}: {plain}");
            }
        }
    }
}
