//! The guide's contents and search, without egui: the table of contents read from `index.md`,
//! the pages cut into blocks of plain text, and a search that ignores case and accents.

/// An area of the table of contents and its pages, (title, id), in reading order.
#[derive(Debug, PartialEq)]
pub struct Area {
    pub name: String,
    pub pages: Vec<(String, String)>,
}

/// The table of contents: `## <area>` and under it `- [<title>](<id>.md)` lines.
pub fn contents(index: &str) -> Vec<Area> {
    let mut areas: Vec<Area> = vec![];
    for line in index.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("## ") {
            areas.push(Area { name: name.trim().to_string(), pages: vec![] });
        } else if let (Some(rest), Some(area)) = (line.strip_prefix("- ["), areas.last_mut())
            && let Some((title, target)) = rest.split_once("](")
            && let Some(id) = target.strip_suffix(".md)")
        {
            area.pages.push((title.to_string(), id.to_string()));
        }
    }
    areas
}

/// The title of a page: its first line, `# Title`.
pub fn title(page: &str) -> &str {
    page.lines().next().and_then(|l| l.strip_prefix("# ")).map_or("", str::trim)
}

/// Letters with an accent and the letter they are searched as.
const ACCENTS: &[(char, &str)] = &[
    ('a', "àáâãäåā"),
    ('c', "çć"),
    ('e', "èéêëē"),
    ('i', "ìíîïī"),
    ('n', "ñ"),
    ('o', "òóôõöøō"),
    ('u', "ùúûüū"),
    ('y', "ýÿ"),
];

/// One character in lower case and without its accent: one character for one, so that a
/// position in the folded text is the same position in the original.
fn fold_char(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    ACCENTS.iter().find(|(_, set)| set.contains(c)).map_or(c, |&(base, _)| base)
}

/// A text in lower case and without accents, as many characters as the original.
pub fn fold(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

/// What the user typed, as it is searched: folded, spaces at the ends dropped and the ones in
/// between made single.
pub fn query(typed: &str) -> String {
    fold(typed).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The regular expression that finds `typed` in the rendered page (egui_commonmark's own search,
/// case-insensitive): each letter also with its accents, everything else literal.
pub fn pattern(typed: &str) -> String {
    let mut out = String::new();
    for c in query(typed).chars() {
        if let Some((base, set)) = ACCENTS.iter().find(|(base, _)| *base == c) {
            out.push('[');
            out.push(*base);
            out.push_str(set);
            out.push(']');
        } else {
            if "\\.+*?()|[]{}^$#&-~".contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
    }
    out
}

/// The text of a markdown line as it is read: no block markers, emphasis, link targets or tags;
/// the text of links and the description of images stay.
pub fn plain(line: &str) -> String {
    let mut s = line.trim_start();
    s = s.trim_start_matches('#').trim_start();
    for marker in ["- ", "* ", "> "] {
        if let Some(rest) = s.strip_prefix(marker) {
            s = rest;
        }
    }
    if let Some(rest) = numbered(s) {
        s = rest;
    }
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            '*' | '`' | '!' if c != '!' || chars.peek() == Some(&'[') => {}
            '|' => out.push(' '),
            '[' => {}
            ']' => {
                // The target of a link or an image: not shown.
                if chars.peek() == Some(&'(') {
                    for t in chars.by_ref() {
                        if t == ')' {
                            break;
                        }
                    }
                }
            }
            '<' if chars.peek().is_some_and(|n| n.is_ascii_alphabetic() || *n == '/') => {
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text after the number of a numbered list item (`3. `).
fn numbered(line: &str) -> Option<&str> {
    let (num, rest) = line.split_once(". ")?;
    (!num.is_empty() && num.bytes().all(|b| b.is_ascii_digit())).then_some(rest)
}

/// Where a match was found: the title first, then a subtitle, then the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Title,
    Heading,
    Text,
}

/// A piece of a page searched as one: a heading, a paragraph, an item of a list, a row of a table.
#[derive(Debug, PartialEq)]
pub struct Block {
    pub kind: Kind,
    pub text: String,
    /// The byte of the page where it starts.
    pub at: usize,
}

/// The page cut into blocks, in reading order; comments (`<!-- … -->`) and rules are left out.
pub fn blocks(page: &str) -> Vec<Block> {
    let mut out: Vec<Block> = vec![];
    let mut open = false; // the last block is a paragraph that the next line continues
    let mut comment = false;
    let mut at = 0;
    for raw in page.split_inclusive('\n') {
        let start = at;
        at += raw.len();
        let line = raw.trim();
        if comment || line.starts_with("<!--") {
            comment = !line.contains("-->");
            open = false;
            continue;
        }
        // Empty lines, rules and the line under the head of a table.
        if line.chars().all(|c| "-|: ".contains(c)) {
            open = false;
            continue;
        }
        let heading = line.starts_with('#');
        let new_item = heading || ["- ", "* ", "|", ">"].iter().any(|m| line.starts_with(m)) || numbered(line).is_some();
        let text = plain(line);
        match out.last_mut() {
            Some(last) if open && !new_item => {
                last.text.push(' ');
                last.text.push_str(&text);
            }
            _ => {
                let kind = match line.bytes().take_while(|&b| b == b'#').count() {
                    0 => Kind::Text,
                    1 => Kind::Title,
                    _ => Kind::Heading,
                };
                out.push(Block { kind, text, at: start });
            }
        }
        open = !heading;
    }
    out
}

/// A result of the search.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: &'static str,
    pub kind: Kind,
    /// The heading for a subtitle, otherwise the sentence with the match, shortened.
    pub excerpt: String,
    /// The bytes of `excerpt` that matched, to show them in bold.
    pub mark: std::ops::Range<usize>,
    /// The byte of the page where the block with the match starts: the highlight goes to the
    /// first match from there.
    pub at: usize,
}

/// Searches `typed` in `pages` (id, text), in their order: in each page the title, every
/// subtitle and the first paragraph that has it. Titles first, then subtitles, then the text.
pub fn search(pages: &[(&'static str, &'static str)], typed: &str) -> Vec<Hit> {
    let q = query(typed);
    if q.is_empty() {
        return vec![];
    }
    let mut hits = vec![];
    for &(id, page) in pages {
        let mut text_found = false;
        for b in blocks(page) {
            if b.kind == Kind::Text && text_found {
                continue;
            }
            let Some(found) = fold(&b.text).find(&q) else { continue };
            // Positions in characters: the folded text has as many as the original.
            let from = fold(&b.text)[..found].chars().count();
            let len = q.chars().count();
            text_found |= b.kind == Kind::Text;
            let (excerpt, mark) = excerpt(&b.text, from, len);
            hits.push(Hit { id, kind: b.kind, excerpt, mark, at: b.at });
        }
    }
    hits.sort_by_key(|h| h.kind); // stable: the order of the pages stays within a kind
    hits
}

/// The sentence of `text` with the match (`from`, `len` in characters), cut to about 140
/// characters around it; returns the excerpt and the bytes of the match in it.
pub fn excerpt(text: &str, from: usize, len: usize) -> (String, std::ops::Range<usize>) {
    const WIDE: usize = 140;
    let chars: Vec<char> = text.chars().collect();
    let end_of = |i: usize| i + 1 < chars.len() && ".!?".contains(chars[i]) && chars[i + 1] == ' ';
    let mut start = (0..from).rev().find(|&i| end_of(i)).map_or(0, |i| i + 2);
    let mut end = (from + len..chars.len()).find(|&i| end_of(i)).map_or(chars.len(), |i| i + 1);
    let (mut lead, mut tail) = ("", "");
    if end - start > WIDE {
        let before = (WIDE.saturating_sub(len)) / 3;
        if from - start > before {
            start = from - before;
            lead = "…";
        }
        if end - start > WIDE {
            end = (start + WIDE).max(from + len);
            tail = "…";
        }
    }
    let piece = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let head = format!("{lead}{}", piece(start, from));
    let found = piece(from, from + len);
    let out = format!("{head}{found}{}{tail}", piece(from + len, end));
    (out, head.len()..head.len() + found.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "# Editing an image\n\nOpen the editor from the viewer. Then choose a tool.\nIt keeps the colours.\n\n## Crop and rotate\n\n- Click ![Crop](icon://crop) to **crop** the [image](viewer.md).\n<!-- shot: editor | overview | the editor -->\n\n| Key | Action |\n|---|---|\n| Ctrl+E | Edit |\n";

    #[test]
    fn contents_of_the_index() {
        let index = "# Contents\n\n## Getting started\n\n- [Getting started](start.md)\n\n## Browsing\n- [The thumbnail grid](grid.md)\n- [Bookmarks](bookmarks.md)\n";
        let areas = contents(index);
        assert_eq!(areas.len(), 2);
        assert_eq!(areas[0], Area { name: "Getting started".into(), pages: vec![("Getting started".into(), "start".into())] });
        assert_eq!(areas[1].pages[1], ("Bookmarks".into(), "bookmarks".into()));
        assert_eq!(title(PAGE), "Editing an image");
    }

    #[test]
    fn folding_keeps_positions() {
        assert_eq!(fold("Città PERCHÉ Ñandú"), "citta perche nandu");
        assert_eq!(fold("àé").chars().count(), 2);
        assert_eq!(query("  Più   VELOCE "), "piu veloce");
        assert_eq!(pattern("Città."), "[cçć][iìíîïī]tt[aàáâãäåā]\\.");
    }

    #[test]
    fn plain_text_of_a_line() {
        assert_eq!(plain("- Click ![Crop](icon://crop) to **crop** the [image](viewer.md)."), "Click Crop to crop the image.");
        assert_eq!(plain("## Crop and rotate"), "Crop and rotate");
        assert_eq!(plain("3. Press `Ctrl+E` <kbd>now</kbd>!"), "Press Ctrl+E now!");
        assert_eq!(plain("| Ctrl+E | Edit |"), "Ctrl+E Edit");
        assert_eq!(plain("2 + 2 \\* 3"), "2 + 2 * 3");
    }

    #[test]
    fn page_in_blocks() {
        let b = blocks(PAGE);
        let texts: Vec<_> = b.iter().map(|b| (b.kind, b.text.as_str())).collect();
        assert_eq!(
            texts,
            vec![
                (Kind::Title, "Editing an image"),
                (Kind::Text, "Open the editor from the viewer. Then choose a tool. It keeps the colours."),
                (Kind::Heading, "Crop and rotate"),
                (Kind::Text, "Click Crop to crop the image."),
                (Kind::Text, "Key Action"),
                (Kind::Text, "Ctrl+E Edit"),
            ]
        );
        assert_eq!(&PAGE[b[2].at..b[2].at + 7], "## Crop");
    }

    #[test]
    fn results_in_order() {
        let other = "# Crop tools\n\nNothing about it.\n\n## More\n\nYou can crop here too. And crop again.\n";
        let hits = search(&[("editor", PAGE), ("other", other)], "CROP");
        let kinds: Vec<_> = hits.iter().map(|h| (h.kind, h.id)).collect();
        // Titles, then subtitles, then the first paragraph of each page with the word.
        assert_eq!(kinds, vec![(Kind::Title, "other"), (Kind::Heading, "editor"), (Kind::Text, "editor"), (Kind::Text, "other")]);
        let last = &hits[3];
        assert_eq!(last.excerpt, "You can crop here too.");
        assert_eq!(&last.excerpt[last.mark.clone()], "crop");
        assert_eq!(&other[last.at..last.at + 7], "You can");
        // Half words, accents and case do not matter.
        assert_eq!(search(&[("editor", PAGE)], "COLOU").len(), 1);
        assert_eq!(search(&[("it", "# Città\n")], "citta")[0].excerpt, "Città");
        assert!(search(&[("editor", PAGE)], "   ").is_empty());
        assert!(search(&[("editor", PAGE)], "shot").is_empty(), "comments are not searched");
    }

    #[test]
    fn long_sentences_are_cut() {
        let text = format!("{} the word {}", "a".repeat(300), "b".repeat(300));
        let from = text.find("word").unwrap();
        let (e, mark) = excerpt(&text, from, 4);
        assert!(e.starts_with('…') && e.ends_with('…'));
        assert!(e.chars().count() <= 142);
        assert_eq!(&e[mark], "word");
        let (e, mark) = excerpt("Città è bella. Qui no.", 0, 5);
        assert_eq!((e.as_str(), mark), ("Città è bella.", 0..6));
    }
}
