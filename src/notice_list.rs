//! The message windows that tell how a work ended (a copy, an export, a turn in the grid), built
//! by one function so that they all read the same (Piero, 21/11): the title says the whole
//! (`told`), each thing to say is a part (`Part`) with a heading in bold that counts its names, a
//! short sentence under it when it helps, and the names as a list, in the order of the grid
//! (`source::natural`), each with why in a few words when it is known. At most `LISTED` names
//! shown, then "and N more" as the last item, and Copy with every name and the whole reasons
//! (Piero, 20/11: the window grew past the screen, and Copy took only the names shown).

use crate::i18n::I18n;
use crate::save_dialogs::{Notice, md, plain};

/// The most names a list shows; the others are counted.
pub(crate) const LISTED: usize = 10;

/// A name as an item of a list (Markdown): a line break in a name ("a\nb.jpg") would split it,
/// so control characters show as �.
pub(crate) fn item(name: &str) -> String {
    md(&name.chars().map(|c| if c.is_control() { '\u{FFFD}' } else { c }).collect::<String>())
}

/// `items` (Markdown, one line each) as a list, the first `LISTED`, then "and N more" as its last
/// item, in line with the names.
pub(crate) fn shortened(i18n: &I18n, items: &[String]) -> String {
    let mut lines: Vec<String> = items.iter().take(LISTED).map(|i| format!("- {i}")).collect();
    if let Some(more) = items.len().checked_sub(LISTED).filter(|&more| more > 0) {
        lines.push(format!("- {}", i18n.t_args("list-more", &[("n", &more.to_string())])));
    }
    lines.join("\n")
}

/// What Copy puts on the clipboard: `intro` (Markdown) as it reads, then each list under its
/// heading (plain text, none when empty) with all its items, one a line.
pub(crate) fn copied(intro: &str, lists: &[(&str, &[String])]) -> String {
    let mut blocks: Vec<String> = Some(plain(intro)).filter(|i| !i.is_empty()).into_iter().collect();
    for (heading, items) in lists {
        let lines = items.iter().map(|i| format!("- {}", plain(i)));
        blocks.push(Some(heading.to_string()).filter(|h| !h.is_empty()).into_iter().chain(lines).collect::<Vec<_>>().join("\n"));
    }
    blocks.join("\n\n")
}

/// A line of a list: a file, a folder, a model.
pub(crate) struct Entry {
    /// Its name, as it is: the list is in the order of the names.
    pub(crate) name: String,
    /// Why, in a few words after the name (plain text); empty: the name alone.
    pub(crate) why: String,
    /// The line as Copy takes it (plain text) when it says more than the list: the whole
    /// reason, the text of a library. `None`: the line as shown.
    pub(crate) whole: Option<String>,
}

impl Entry {
    /// The line as the list shows it (Markdown).
    fn shown(&self) -> String {
        if self.why.is_empty() { item(&self.name) } else { format!("{}: {}", item(&self.name), md(&self.why)) }
    }
}

/// One of the things a message has to say, about some names.
pub(crate) struct Part {
    /// Said in bold over the names, with how many they are ("4 models not exported:").
    heading: String,
    /// A short sentence under the heading (Markdown); empty: none.
    why: String,
    /// In the order of the names.
    entries: Vec<Entry>,
}

impl Part {
    /// The part about `entries`, its words by how many they are (`$n`): the Fluent ids of the
    /// heading and of the sentence under it (`""`: none).
    pub(crate) fn of(i18n: &I18n, heading: &str, why: &str, mut entries: Vec<Entry>) -> Part {
        let n = entries.len().to_string();
        let say = |id: &str| if id.is_empty() { String::new() } else { i18n.t_args(id, &[("n", n.as_str())]) };
        entries.sort_by(|a, b| crate::source::natural(&a.name, &b.name));
        Part { heading: say(heading), why: say(why), entries }
    }

    /// `of`, about names alone.
    pub(crate) fn named(i18n: &I18n, heading: &str, why: &str, names: &[String]) -> Part {
        Part::of(i18n, heading, why, names.iter().map(|n| Entry { name: n.clone(), why: String::new(), whole: None }).collect())
    }

    /// The heading and the sentence under it, each on its line: `bold` as the window shows them
    /// (Markdown), otherwise as Copy takes them.
    fn head(&self, bold: bool) -> String {
        let heading = if bold { format!("**{}**", md(&self.heading)) } else { self.heading.clone() };
        let why = if bold { self.why.clone() } else { plain(&self.why) };
        if why.is_empty() { heading } else { format!("{heading}\n{why}") }
    }
}

/// The message at the end of a work: `title` says the whole ("Copy finished"), `intro`
/// (Markdown, may be empty) what was done, then the parts that have names, each under its
/// heading; with the warning triangle when there is one. Copy takes every name.
pub(crate) fn told(i18n: &I18n, title: String, intro: &str, parts: Vec<Part>) -> Notice {
    let parts: Vec<Part> = parts.into_iter().filter(|p| !p.entries.is_empty()).collect();
    if parts.is_empty() {
        return Notice::new(title, intro.to_string());
    }
    let shown = parts.iter().map(|p| {
        let lines: Vec<String> = p.entries.iter().map(Entry::shown).collect();
        format!("{}\n{}", p.head(true), shortened(i18n, &lines))
    });
    let text = Some(intro.to_string()).filter(|i| !i.is_empty()).into_iter().chain(shown).collect::<Vec<_>>().join("\n\n");
    // Copy takes its lines as plain text: the Markdown of a name is put back for `copied`.
    let whole: Vec<(String, Vec<String>)> = parts.iter().map(|p| (p.head(false), p.entries.iter().map(|e| e.whole.as_deref().map_or_else(|| e.shown(), md)).collect())).collect();
    let lists: Vec<(&str, &[String])> = whole.iter().map(|(h, lines)| (h.as_str(), lines.as_slice())).collect();
    Notice { copy: Some(copied(intro, &lists)), warning: true, ..Notice::new(title, text) }
}

/// `told`, with a last line that says the guide has more ("More in the guide (F1)."), also in
/// Copy: F1 opens `page` while the message is up (`Notice::help`).
pub(crate) fn with_guide(i18n: &I18n, told: Notice, page: &'static str) -> Notice {
    let more = i18n.t("notice-guide");
    let copy = told.copy.map(|c| format!("{c}\n\n{more}"));
    Notice { text: format!("{}\n\n{more}", told.text), copy, help: Some(page), ..told }
}

/// The message at the end of a turn or a flip in the grid (`title`, and `done`: what was saved),
/// with the pictures it left out: those that cannot be saved over themselves (`skipped`:
/// animated, with several pages…) and those too large for the memory (`large`, #177).
pub(crate) fn turned(i18n: &I18n, title: String, done: &str, skipped: &[String], large: &[String]) -> Notice {
    told(i18n, title, done, vec![Part::named(i18n, "batch-skipped-heading", "batch-skipped-why", skipped), Part::named(i18n, "summary-too-large", "", large)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("dir_{i}/a*{i}.jpg")).collect()
    }

    /// Ten names shown whole; eleven: ten and "and 1 more", in the singular, as the last item of
    /// the list; fifteen: "and 5 more"; in both languages.
    #[test]
    fn ten_then_more() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let items = |n: usize| names(n).iter().map(|s| item(s)).collect::<Vec<_>>();
        let ten = shortened(&en, &items(10));
        assert_eq!(ten.lines().count(), 10);
        assert!(ten.ends_with(&format!("- {}", md("dir_10/a*10.jpg"))) && !ten.contains("more"), "{ten}");
        assert_eq!(shortened(&en, &items(11)).matches("\n- ").count(), 10);
        assert!(shortened(&en, &items(11)).ends_with(&format!("- {}\n- and 1 more", md("dir_10/a*10.jpg"))));
        assert!(shortened(&en, &items(15)).ends_with("\n- and 5 more"));
        assert!(shortened(&it, &items(11)).ends_with("\n- e un altro"));
        assert!(shortened(&it, &items(15)).ends_with("\n- e altri 5"));
        assert_eq!(shortened(&it, &[]), "");
    }

    /// Copy takes the lists under a heading each, with every item, as the summary of Convert…
    /// has them; the text before them as it reads.
    #[test]
    fn copy_under_headings() {
        let (a, b) = (vec![md("x.txt")], vec![md("y.heic"), md("z.raw")]);
        assert_eq!(copied("**2** done.", &[("Kind:", &a), ("Format:", &b)]), "2 done.\n\nKind:\n- x.txt\n\nFormat:\n- y.heic\n- z.raw");
        assert_eq!(copied("", &[("Kind:", &a)]), "Kind:\n- x.txt");
    }

    /// The pictures left out of a turn in the grid: a part under each heading with its count, an
    /// empty one left out, ten names each at most, in their order; Copy has them all. With
    /// nothing left out, the message of what was done, without the triangle.
    #[test]
    fn left_out_of_a_turn() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let large: Vec<String> = names(12).into_iter().rev().collect();
        let n = turned(&en, "Rotated".into(), "**12** done.", &[], &large);
        assert!(n.warning && n.title == "Rotated");
        assert!(plain(&n.text).starts_with("12 done.\n\n12 pictures too large for the memory of this computer, left out:\n- dir_1/a*1.jpg\n- dir_2/a*2.jpg\n") && !n.text.contains("animated"), "{}", n.text);
        assert!(n.text.ends_with("\n- and 2 more") && n.text.contains("**12 pictures too large"), "{}", n.text);
        let copy = n.copy.unwrap();
        assert!(copy.starts_with("12 done.\n\n12 pictures too large for the memory of this computer, left out:\n- dir_1/a*1.jpg") && copy.ends_with("- dir_11/a*11.jpg\n- dir_12/a*12.jpg"), "{copy}");
        let both = turned(&it, "Immagini ruotate".into(), "**2 immagini** ruotate.", &["b.gif".into(), "a.tif".into()], &["c.cr3".into()]);
        assert_eq!(plain(&both.text), "2 immagini ruotate.\n\n2 immagini saltate:\nSono animate, multipagina, foto in movimento o con una mappa di profondità: Salva non le riscrive.\n- a.tif\n- b.gif\n\n1 immagine troppo grande per la memoria di questo computer, lasciata fuori:\n- c.cr3");
        let one = turned(&en, String::new(), "x", &["a.gif".into()], &[]);
        assert_eq!(plain(&one.text), "x\n\n1 picture left out:\nIt is animated, has several pages, is a motion photo or carries a depth map: Save does not write it again.\n- a.gif");
        assert_eq!(plain(&turned(&en, String::new(), "x", &["a.gif".into(), "b.gif".into()], &[]).text).lines().nth(3), Some("They are animated, have several pages, are motion photos or carry a depth map: Save does not write them again."));
        let none = turned(&en, "Rotated".into(), "**3** done.", &[], &[]);
        assert!(!none.warning && none.copy.is_none() && none.text == "**3** done.");
    }

    /// A message with parts (`told`), as the end of an export has it: the heading of each part in
    /// bold with its count, the sentence under it, the names in their natural order whatever
    /// order they came in, why after a name, "and N more" as the last item; an empty part left
    /// out; the triangle. Copy with every name and the whole reasons, without Markdown.
    #[test]
    fn parts_under_their_headings() {
        let it = I18n::new("it");
        let failed = vec![
            Entry { name: "senza-bin.gltf".into(), why: "manca il file senza-bin.bin".into(), whole: Some("Impossibile aprire senza-bin.gltf: Manca il file con i dati del modello: senza-bin.bin".into()) },
            Entry { name: "Dati_rovinati.gltf".into(), why: String::new(), whole: Some("Impossibile aprire Dati_rovinati.gltf (bad *base64*)".into()) },
        ];
        let again: Vec<String> = ["nodo-doppio-10.gltf", "nodo-doppio-02.gltf", "nodo-doppio.gltf"].into_iter().map(String::from).chain((3..=9).rev().map(|i| format!("nodo-doppio-0{i}.gltf"))).chain(["nodo-doppio-12.gltf".to_string(), "nodo-doppio-11.gltf".to_string()]).collect();
        let parts = vec![Part::of(&it, "model-export-failed-heading", "", failed), Part::named(&it, "model-export-failed-heading", "", &[]), Part::named(&it, "model-export-again-heading", "model-export-again-why", &again)];
        let n = told(&it, "Esportazione finita".into(), "", parts);
        assert!(n.warning && n.title == "Esportazione finita");
        let blocks: Vec<&str> = n.text.split("\n\n").collect();
        assert_eq!(blocks.len(), 2, "{}", n.text);
        assert_eq!(blocks[0], format!("**2 modelli non esportati{}**\n- {}\n- {}: manca il file senza\\-bin\\.bin", md(":"), md("Dati_rovinati.gltf"), md("senza-bin.gltf")));
        let shown: Vec<String> = [String::new()].into_iter().chain((2..=10).map(|i| format!("-{i:02}"))).map(|i| format!("- {}", md(&format!("nodo-doppio{i}.gltf")))).collect();
        let why = "Mettevano una loro parte più volte nella gerarchia, cosa che glTF non permette: nei file nuovi c'è una volta sola.";
        assert_eq!(blocks[1], format!("**12 modelli esportati con una modifica{}**\n{why}\n{}\n- e altri 2", md(":"), shown.join("\n")));
        let copy = n.copy.unwrap();
        let all: Vec<String> = [String::new()].into_iter().chain((2..=12).map(|i| format!("-{i:02}"))).map(|i| format!("- nodo-doppio{i}.gltf")).collect();
        assert_eq!(copy, format!("2 modelli non esportati:\n- Impossibile aprire Dati_rovinati.gltf (bad *base64*)\n- Impossibile aprire senza-bin.gltf: Manca il file con i dati del modello: senza-bin.bin\n\n12 modelli esportati con una modifica:\n{why}\n{}", all.join("\n")));
        // One name: the singular; what was done first, as it was written.
        let one = told(&I18n::new("en"), "Export finished".into(), "**3** done.", vec![Part::named(&I18n::new("en"), "model-export-again-heading", "", &["a\nb.gltf".to_string()])]);
        assert_eq!(one.text, format!("**3** done.\n\n**1 model exported with a change{}**\n- a\u{FFFD}b{}", md(":"), md(".gltf")));
        assert_eq!(one.copy.as_deref(), Some("3 done.\n\n1 model exported with a change:\n- a\u{FFFD}b.gltf"));
        // Nothing to list: the message of what was done, without the triangle.
        let none = told(&it, "Titolo".into(), "Fatto.", vec![Part::named(&it, "model-export-again-heading", "", &[])]);
        assert!(!none.warning && none.copy.is_none() && none.text == "Fatto.");
    }
}
