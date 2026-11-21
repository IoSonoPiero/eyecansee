//! The lists of names in the message windows (files not copied for their name, pictures skipped
//! by a turn in the grid, files left out by Convert…): at most `LISTED` names shown, then "and N
//! more", and Copy with every name (Piero, 20/11: the window grew past the screen, and Copy took
//! only the names shown).

use crate::i18n::I18n;
use crate::save_dialogs::{Notice, md, plain};

/// The most names a message lists; the others are counted.
pub(crate) const LISTED: usize = 10;

/// A name as an item of a list (Markdown): a line break in a name ("a\nb.jpg") would split it,
/// so control characters show as �.
pub(crate) fn item(name: &str) -> String {
    md(&name.chars().map(|c| if c.is_control() { '\u{FFFD}' } else { c }).collect::<String>())
}

/// `items` (Markdown, one line each) as a list, the first `LISTED`, then "and N more" in a
/// paragraph of its own.
pub(crate) fn shortened(i18n: &I18n, items: &[String]) -> String {
    let list = items.iter().take(LISTED).map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n");
    match items.len().checked_sub(LISTED).filter(|&more| more > 0) {
        Some(more) => format!("{list}\n\n{}", i18n.t_args("list-more", &[("n", &more.to_string())])),
        None => list,
    }
}

/// What Copy puts on the clipboard: `intro` (Markdown) as it reads, then each list under its
/// heading (plain text, none when empty) with all its items, one a line.
pub(crate) fn copied(intro: &str, lists: &[(&str, &[String])]) -> String {
    let mut text = plain(intro);
    for (heading, items) in lists {
        text.push_str("\n\n");
        if !heading.is_empty() {
            text.push_str(heading);
            text.push('\n');
        }
        let lines: Vec<String> = items.iter().map(|i| format!("- {}", plain(i))).collect();
        text.push_str(&lines.join("\n"));
    }
    text
}

/// A message about `names` (as they are): `intro` (Markdown), then the names, shortened
/// (`shortened`); Copy takes them all (`copied`).
pub(crate) fn notice(i18n: &I18n, title: String, intro: &str, names: &[String], warning: bool) -> Notice {
    let items: Vec<String> = names.iter().map(|n| item(n)).collect();
    let text = format!("{intro}\n\n{}", shortened(i18n, &items));
    Notice { copy: Some(copied(intro, &[("", &items)])), warning, ..Notice::new(title, text) }
}

/// A warning about `names` whose words go by how many they are (`$n`): the title `title`, the
/// sentences `lines` (Fluent ids) one a line, then the names (`notice`). The names a copy left
/// out, what a move left in its folders, the models exported with a change.
pub(crate) fn counted(i18n: &I18n, title: &str, lines: &[&str], names: &[String]) -> Notice {
    let n = names.len().to_string();
    let args = [("n", n.as_str())];
    let intro = lines.iter().map(|id| i18n.t_args(id, &args)).collect::<Vec<_>>().join("\n");
    notice(i18n, i18n.t_args(title, &args), &intro, names, true)
}

/// As `notice`, with several lists of names, each under its heading, the empty ones left out:
/// the pictures left out of a Save on the grid, animated or too large for the memory (#177).
pub(crate) fn notice_lists(i18n: &I18n, title: String, intro: &str, lists: &[(String, Vec<String>)]) -> Notice {
    let lists: Vec<(&str, Vec<String>)> = lists.iter().filter(|(_, names)| !names.is_empty()).map(|(h, names)| (h.as_str(), names.iter().map(|n| item(n)).collect())).collect();
    let text = lists.iter().fold(intro.to_string(), |text, (heading, items)| format!("{text}\n\n{heading}\n\n{}", shortened(i18n, items)));
    let refs: Vec<(&str, &[String])> = lists.iter().map(|(h, items)| (*h, items.as_slice())).collect();
    Notice { copy: Some(copied(intro, &refs)), ..Notice::new(title, text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("dir_{i}/a*{i}.jpg")).collect()
    }

    /// Ten names shown whole; eleven: ten and "and 1 more", in the singular; fifteen: "and 5
    /// more"; in both languages.
    #[test]
    fn ten_then_more() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let items = |n: usize| names(n).iter().map(|s| item(s)).collect::<Vec<_>>();
        let ten = shortened(&en, &items(10));
        assert_eq!(ten.lines().count(), 10);
        assert!(ten.ends_with(&format!("- {}", md("dir_10/a*10.jpg"))) && !ten.contains("more"), "{ten}");
        assert_eq!(shortened(&en, &items(11)).matches("\n- ").count(), 9);
        assert!(shortened(&en, &items(11)).ends_with(&format!("- {}\n\nand 1 more", md("dir_10/a*10.jpg"))));
        assert!(shortened(&en, &items(15)).ends_with("\n\nand 5 more"));
        assert!(shortened(&it, &items(11)).ends_with("\n\ne un altro"));
        assert!(shortened(&it, &items(15)).ends_with("\n\ne altri 5"));
        assert_eq!(shortened(&it, &[]), "");
    }

    /// Copy takes every name, as it is (no Markdown escapes), each on its line, under the text.
    #[test]
    fn copy_takes_them_all() {
        let it = I18n::new("it");
        let n = notice(&it, "Titolo".into(), "**15 file** saltati:", &names(15), true);
        assert!(n.warning && n.save_as.is_none());
        assert!(n.text.ends_with("\n\ne altri 5"), "{}", n.text);
        let copy = n.copy.unwrap();
        let want: Vec<String> = names(15).iter().map(|s| format!("- {s}")).collect();
        assert_eq!(copy, format!("15 file saltati:\n\n{}", want.join("\n")));
        assert!(!copy.contains("altri") && !copy.contains('\\'));
        // A name with a line break stays one line.
        let broken = notice(&it, String::new(), "x", &["a\nb.jpg".to_string()], false);
        assert_eq!(broken.copy.as_deref(), Some("x\n\n- a\u{FFFD}b.jpg"));
        assert!(broken.text.ends_with(&format!("- a\u{FFFD}b{}", md(".jpg"))));
        // Lists under a heading each, as the summary of Convert… has them.
        let (a, b) = (vec![md("x.txt")], vec![md("y.heic"), md("z.raw")]);
        assert_eq!(copied("**2** done.", &[("Kind:", &a), ("Format:", &b)]), "2 done.\n\nKind:\n- x.txt\n\nFormat:\n- y.heic\n- z.raw");
    }

    /// The pictures left out of a Save on the grid: a list under each heading, an empty one left
    /// out, ten names each at most; Copy has them all.
    #[test]
    fn lists_under_their_headings() {
        let en = I18n::new("en");
        let lists = [("Animated:".to_string(), Vec::new()), ("Too large:".to_string(), names(12))];
        let n = notice_lists(&en, "Rotated".into(), "**12** done.", &lists);
        assert!(n.text.starts_with("**12** done.\n\nToo large:\n\n- ") && !n.text.contains("Animated"), "{}", n.text);
        assert!(n.text.ends_with("\n\nand 2 more"), "{}", n.text);
        let copy = n.copy.unwrap();
        assert!(copy.starts_with("12 done.\n\nToo large:\n- dir_1/a*1.jpg") && copy.ends_with("- dir_12/a*12.jpg"), "{copy}");
        let both = notice_lists(&en, String::new(), "x", &[("A:".into(), vec!["a.gif".into()]), ("B:".into(), vec!["b.tif".into()])]);
        assert_eq!(both.text, format!("x\n\nA:\n\n- {}\n\nB:\n\n- {}", md("a.gif"), md("b.tif")));
    }
}
