//! Checks of the illustrated guide in `docs/help` (issue #34): the same pages and headings in
//! every language, the contents page, links, images, icons, image placeholders, the pages and
//! images embedded in the program and the F1 spots (`"help:<id>"`) written in the code.
//! Each check is a pure function (text in, list of problems out), tested on small fake pages;
//! `guide_is_consistent` runs them all on the real guide.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod stubs; // stub until guida-A/guida-C merge
use stubs::{IMAGES, PAGES, icon_exists};

/// Languages of the guide, in the order of `Area::names`, with the title of the contents page.
const LANGS: &[(&str, &str)] = &[("en", "Contents"), ("it", "Sommario")];

/// One group of the contents page: its name in each language of `LANGS` and its pages in order.
struct Area {
    names: [&'static str; 2],
    ids: &'static [&'static str],
}

/// The pages of the guide (notes/guida-contratto.md, point 1), in reading order.
const CONTRACT: &[Area] = &[
    Area { names: ["Getting started", "Per cominciare"], ids: &["start"] },
    Area { names: ["Browsing", "Navigare"], ids: &["grid", "folders", "address", "bookmarks", "selection"] },
    Area {
        names: ["Viewing", "Guardare"],
        ids: &["viewer", "metadata", "slideshow", "video", "model-3d", "model-export", "cad"],
    },
    Area {
        names: ["Editing", "Modificare"],
        ids: &[
            "editor",
            "editor-marquee",
            "editor-lines",
            "editor-draw",
            "editor-text",
            "editor-lens",
            "editor-stamps",
            "editor-cover",
            "editor-filters",
            "editor-crop",
            "editor-colors",
            "editor-save",
            "rotate",
        ],
    },
    Area { names: ["Saving and converting", "Salvare e convertire"], ids: &["save-as", "convert", "convert-video"] },
    Area { names: ["Files", "File"], ids: &["copy-move", "rename", "delete", "new-image", "duplicates"] },
    Area {
        names: ["Devices and servers", "Dispositivi e server"],
        ids: &["devices", "phone", "servers", "server-manager", "passwords"],
    },
    Area { names: ["Settings", "Impostazioni"], ids: &["options", "updates", "components"] },
    Area { names: ["Reference", "Riferimento"], ids: &["keyboard", "formats", "limits"] },
];

/// The guide as read from disk.
#[derive(Default)]
struct Docs {
    /// Language → page id (file name without `.md`) → text.
    pages: BTreeMap<String, BTreeMap<String, String>>,
    /// Names of the files in `img/`, without `.webp`.
    images: BTreeSet<String>,
    /// What could not be read, or files that do not belong.
    problems: Vec<String>,
}

/// What the checks found: problems make the test fail, pending placeholders are only listed.
#[derive(Default)]
struct Report {
    problems: Vec<String>,
    pending: Vec<String>,
}

/// Where a problem is, as it is written in the messages.
fn at(lang: &str, id: &str) -> String {
    format!("docs/help/{lang}/{id}.md")
}

/// The files of a folder as (name, path), sorted; a missing folder is a problem.
fn files(dir: &Path, problems: &mut Vec<String>) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        problems.push(format!("{}: folder missing", dir.display()));
        return vec![];
    };
    let mut out: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    out.sort();
    out
}

/// Reads `<root>/<lang>/*.md` for every language and the names of `<root>/img/*.webp`.
fn load(root: &Path) -> Docs {
    let mut docs = Docs::default();
    for &(lang, _) in LANGS {
        let mut pages = BTreeMap::new();
        for (name, path) in files(&root.join(lang), &mut docs.problems) {
            let Some(id) = name.strip_suffix(".md") else {
                docs.problems.push(format!("{}: not a .md page", path.display()));
                continue;
            };
            match std::fs::read_to_string(&path) {
                Ok(text) => _ = pages.insert(id.to_string(), text),
                Err(e) => docs.problems.push(format!("{}: {e}", path.display())),
            }
        }
        docs.pages.insert(lang.to_string(), pages);
    }
    for (name, path) in files(&root.join("img"), &mut docs.problems) {
        match name.strip_suffix(".webp") {
            Some(stem) => _ = docs.images.insert(stem.to_string()),
            None => docs.problems.push(format!("{}: images are .webp", path.display())),
        }
    }
    docs
}

/// The lines outside fenced code blocks, numbered from 1.
fn prose(md: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut fenced = false;
    md.lines().enumerate().filter_map(move |(i, line)| {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            return None;
        }
        (!fenced).then_some((i + 1, line))
    })
}

/// The headings `#`, `##`, … of a page as (line, level, title).
fn headings(md: &str) -> Vec<(usize, usize, &str)> {
    prose(md)
        .filter_map(|(n, line)| {
            let level = line.bytes().take_while(|&b| b == b'#').count();
            let title = line[level..].strip_prefix(' ')?;
            (level > 0).then(|| (n, level, title.trim()))
        })
        .collect()
}

/// The title of a page: its first line, `# Title`.
fn title(md: &str) -> Option<&str> {
    headings(md).first().filter(|h| h.0 == 1 && h.1 == 1).map(|h| h.2)
}

/// A page starts with `# Title`, has no other `#` and nothing deeper than `###`.
fn check_headings(file: &str, md: &str) -> Vec<String> {
    let mut out = vec![];
    if title(md).is_none() {
        out.push(format!("{file}:1: the first line must be the title, `# Title`"));
    }
    for (n, level, _) in headings(md).into_iter().skip(1) {
        if level == 1 {
            out.push(format!("{file}:{n}: a second `#` title; sections are `##`"));
        } else if level > 3 {
            out.push(format!("{file}:{n}: heading deeper than `###`"));
        }
    }
    out
}

/// The same headings, by level and order, as the English page.
fn check_same_headings(file: &str, en: &str, other: &str) -> Vec<String> {
    let (a, b) = (headings(en), headings(other));
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if x.1 != y.1 {
            return vec![format!(
                "{file}:{}: heading {} is level {} but level {} in English (line {})",
                y.0,
                i + 1,
                y.1,
                x.1,
                x.0
            )];
        }
    }
    if a.len() != b.len() {
        return vec![format!("{file}: {} headings, the English page has {}", b.len(), a.len())];
    }
    vec![]
}

/// What a link or an image points to.
#[derive(Debug, PartialEq)]
enum Target<'a> {
    Page(&'a str),
    Image(&'a str),
    Icon(&'a str),
    External,
    Bad(&'static str),
}

fn classify(target: &str) -> Target<'_> {
    if let Some(name) = target.strip_prefix("icon://") {
        Target::Icon(name)
    } else if target.starts_with("https://") || target.starts_with("http://") {
        Target::External
    } else if target.contains('#') {
        Target::Bad("anchors are not supported, link the whole page")
    } else if let Some(name) = target.strip_prefix("../img/") {
        name.strip_suffix(".webp").map_or(Target::Bad("images are ../img/<name>.webp"), Target::Image)
    } else if let Some(id) = target.strip_suffix(".md")
        && !id.is_empty()
        && !id.contains('/')
    {
        Target::Page(id)
    } else {
        Target::Bad("expected <id>.md, ../img/<name>.webp, icon://<name> or https://…")
    }
}

/// The targets of the links and images `](target)` of a page as (line, target).
fn targets(md: &str) -> Vec<(usize, &str)> {
    prose(md)
        .flat_map(|(n, line)| {
            line.split("](").skip(1).map(move |rest| (n, rest.split([')', ' ']).next().unwrap_or("")))
        })
        .collect()
}

/// The images a page shows, in order.
fn cited_images(md: &str) -> Vec<&str> {
    targets(md)
        .into_iter()
        .filter_map(|(_, t)| match classify(t) {
            Target::Image(name) => Some(name),
            _ => None,
        })
        .collect()
}

/// Every link leads to a page, every image to a file of `img/`, every icon to a known icon.
fn check_targets(
    file: &str,
    md: &str,
    pages: &BTreeMap<String, String>,
    images: &BTreeSet<String>,
    icon_exists: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let mut out = vec![];
    for (n, t) in targets(md) {
        match classify(t) {
            Target::Page(id) if !pages.contains_key(id) => out.push(format!("{file}:{n}: link to a missing page, {id}.md")),
            Target::Image(name) if !images.contains(name) => {
                out.push(format!("{file}:{n}: image {name}.webp is not in docs/help/img"))
            }
            Target::Icon(name) if !icon_exists(name) => out.push(format!("{file}:{n}: unknown icon icon://{name}")),
            Target::Bad(why) => out.push(format!("{file}:{n}: link ({t}): {why}")),
            _ => {}
        }
    }
    out
}

/// The image placeholders `<!-- shot: <name> | … -->` of a page as (line, name).
fn shots(md: &str) -> Vec<(usize, &str)> {
    prose(md)
        .filter_map(|(n, line)| {
            let rest = line.trim().strip_prefix("<!-- shot:")?;
            let name = rest.split('|').next().unwrap_or("");
            Some((n, name.trim().trim_end_matches("-->").trim()))
        })
        .collect()
}

/// A placeholder whose image exists is an error; the others are listed as pending.
fn check_shots(file: &str, md: &str, images: &BTreeSet<String>, report: &mut Report) {
    for (n, name) in shots(md) {
        if images.contains(name) {
            report.problems.push(format!(
                "{file}:{n}: placeholder for {name}.webp, which exists: write ![…](../img/{name}.webp) instead"
            ));
        } else {
            report.pending.push(format!("{file}:{n}: {name}"));
        }
    }
}

/// Every language has exactly the pages of the contract and `index.md`.
fn check_page_set(docs: &Docs, contract: &[Area]) -> Vec<String> {
    let expected: BTreeSet<&str> = contract.iter().flat_map(|a| a.ids.iter().copied()).chain(["index"]).collect();
    let mut out = vec![];
    for (lang, pages) in &docs.pages {
        for id in &expected {
            if !pages.contains_key(*id) {
                out.push(format!("{}: missing", at(lang, id)));
            }
        }
        for id in pages.keys().filter(|id| !expected.contains(id.as_str())) {
            out.push(format!("{}: not a page of the contract (notes/guida-contratto.md)", at(lang, id)));
        }
    }
    out
}

/// One line `- [Title](id.md)` of the contents page.
struct Entry<'a> {
    line: usize,
    title: &'a str,
    id: &'a str,
}

/// An area of the contents page: its name and its entries.
type IndexArea<'a> = (&'a str, Vec<Entry<'a>>);

/// The contents page as its title and its areas; other lines are problems.
fn parse_index<'a>(file: &str, md: &'a str) -> (Option<&'a str>, Vec<IndexArea<'a>>, Vec<String>) {
    let (mut top, mut areas, mut out) = (None, Vec::<IndexArea>::new(), vec![]);
    for (n, line) in prose(md).filter(|(_, l)| !l.trim().is_empty()) {
        let entry = line.strip_prefix("- [").and_then(|rest| rest.split_once("](")).and_then(|(title, rest)| {
            let id = rest.strip_suffix(".md)")?;
            Some(Entry { line: n, title, id })
        });
        if let Some(name) = line.strip_prefix("## ") {
            areas.push((name.trim(), vec![]));
        } else if let Some(t) = line.strip_prefix("# ").filter(|_| n == 1) {
            top = Some(t.trim());
        } else if let (Some(e), Some(area)) = (entry, areas.last_mut()) {
            area.1.push(e);
        } else {
            out.push(format!("{file}:{n}: unexpected line; the contents page has only `#`, `## Area` and `- [Title](id.md)`"));
        }
    }
    (top, areas, out)
}

/// The contents page of one language: its title, the areas and pages of the contract in order,
/// every page once, and the title of each entry equal to the title of its page.
fn check_index(lang_ix: usize, pages: &BTreeMap<String, String>, contract: &[Area]) -> Vec<String> {
    let (lang, contents) = LANGS[lang_ix];
    let file = at(lang, "index");
    let Some(md) = pages.get("index") else { return vec![] };
    let (top, areas, mut out) = parse_index(&file, md);
    if top != Some(contents) {
        out.push(format!("{file}:1: the first line must be `# {contents}`"));
    }
    let mut seen = BTreeMap::<&str, usize>::new();
    for e in areas.iter().flat_map(|a| &a.1) {
        *seen.entry(e.id).or_default() += 1;
        match pages.get(e.id).and_then(|p| title(p)) {
            Some(t) if t != e.title => out.push(format!("{file}:{}: \"{}\" but {}.md is titled \"{t}\"", e.line, e.title, e.id)),
            None if !pages.contains_key(e.id) => {
                out.push(format!("{file}:{}: lists {}.md, which does not exist", e.line, e.id))
            }
            _ => {}
        }
    }
    for (id, n) in seen.iter().filter(|(_, n)| **n > 1) {
        out.push(format!("{file}: lists {id}.md {n} times"));
    }
    for id in pages.keys().filter(|id| *id != "index" && !seen.contains_key(id.as_str())) {
        out.push(format!("{file}: does not list {id}.md"));
    }
    let found: Vec<(&str, Vec<&str>)> = areas.iter().map(|(name, es)| (*name, es.iter().map(|e| e.id).collect())).collect();
    let wanted: Vec<(&str, Vec<&str>)> = contract.iter().map(|a| (a.names[lang_ix], a.ids.to_vec())).collect();
    if found != wanted {
        out.push(format!("{file}: areas and pages differ from the contract:\n  found  {found:?}\n  wanted {wanted:?}"));
    }
    out
}

/// One page in every language: headings, links, icons, images and placeholders, compared with English.
fn check_page(docs: &Docs, id: &str, icon_exists: &dyn Fn(&str) -> bool, report: &mut Report) {
    let en = docs.pages.get("en").and_then(|p| p.get(id));
    for (lang, pages) in &docs.pages {
        let (file, Some(md)) = (at(lang, id), pages.get(id)) else { continue };
        report.problems.extend(check_headings(&file, md));
        report.problems.extend(check_targets(&file, md, pages, &docs.images, icon_exists));
        check_shots(&file, md, &docs.images, report);
        let Some(en) = en.filter(|_| lang != "en") else { continue };
        report.problems.extend(check_same_headings(&file, en, md));
        let (a, b) = (cited_images(en), cited_images(md));
        if a != b {
            report.problems.push(format!("{file}: shows the images {b:?}, the English page {a:?}"));
        }
        let (a, b): (Vec<_>, Vec<_>) = (shots(en).iter().map(|s| s.1).collect(), shots(md).iter().map(|s| s.1).collect());
        if a != b {
            report.problems.push(format!("{file}: has the placeholders {b:?}, the English page {a:?}"));
        }
    }
}

/// All the checks of the files of the guide: pages, headings, contents, links, images, icons
/// and placeholders.
fn check_guide(docs: &Docs, contract: &[Area], icon_exists: &dyn Fn(&str) -> bool) -> Report {
    let mut report = Report { problems: docs.problems.clone(), pending: vec![] };
    report.problems.extend(check_page_set(docs, contract));
    let ids: BTreeSet<&str> = docs.pages.values().flat_map(|p| p.keys().map(String::as_str)).collect();
    for id in ids {
        check_page(docs, id, icon_exists, &mut report);
    }
    for (ix, &(lang, _)) in LANGS.iter().enumerate() {
        if let Some(pages) = docs.pages.get(lang) {
            report.problems.extend(check_index(ix, pages, contract));
        }
    }
    let cited: BTreeSet<&str> = docs.pages.values().flat_map(|p| p.values()).flat_map(|md| cited_images(md)).collect();
    for name in docs.images.iter().filter(|n| !cited.contains(n.as_str())) {
        report.problems.push(format!("docs/help/img/{name}.webp: not shown by any page"));
    }
    report
}

/// The pages and images embedded in the program are exactly the files of the guide.
fn check_embedded(docs: &Docs, pages: &[(&str, &str)], images: &[&str]) -> Vec<String> {
    let mut out = vec![];
    for &(lang, id) in pages {
        if !docs.pages.get(lang).is_some_and(|p| p.contains_key(id)) {
            out.push(format!("help::PAGES has {lang}/{id}, but {} does not exist", at(lang, id)));
        }
    }
    for (lang, files) in &docs.pages {
        for id in files.keys().filter(|id| !pages.contains(&(lang.as_str(), id.as_str()))) {
            out.push(format!("{}: not embedded in help::PAGES", at(lang, id)));
        }
    }
    for name in images.iter().filter(|n| !docs.images.contains(**n)) {
        out.push(format!("help::IMAGES has {name}, but docs/help/img/{name}.webp does not exist"));
    }
    for name in docs.images.iter().filter(|n| !images.contains(&n.as_str())) {
        out.push(format!("docs/help/img/{name}.webp: not embedded in help::IMAGES"));
    }
    out
}

/// The F1 spots `"help:<id>"` of a source text as (line, id).
fn spots(text: &str) -> Vec<(usize, &str)> {
    let plain = |id: &str| !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    text.lines()
        .enumerate()
        .flat_map(|(i, line)| {
            line.split("\"help:").skip(1).filter_map(move |rest| {
                let id = rest.split('"').next()?;
                plain(id).then_some((i + 1, id))
            })
        })
        .collect()
}

/// Every F1 spot written in the sources has its page in every language.
fn check_spots(sources: &[(String, String)], docs: &Docs) -> Vec<String> {
    let mut out = vec![];
    for (path, text) in sources {
        for (n, id) in spots(text) {
            for lang in docs.pages.iter().filter(|(_, p)| !p.contains_key(id)).map(|(lang, _)| lang) {
                out.push(format!("{path}:{n}: \"help:{id}\" has no {}", at(lang, id)));
            }
        }
    }
    out
}

/// The `.rs` files of `src/` as (path from the crate root, text), without this checker.
fn sources(root: &Path) -> Vec<(String, String)> {
    let mut dirs = vec![root.join("src")];
    let mut out = vec![];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()) {
            let name = entry.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            if entry.is_dir() {
                dirs.push(entry);
            } else if name.ends_with(".rs") && !name.contains("help_check") {
                out.push((name, std::fs::read_to_string(&entry).unwrap()));
            }
        }
    }
    out
}

/// The whole guide: files, contents, links, images, icons, placeholders, embedded tables, F1 spots.
#[test]
#[ignore = "until the guida merge: needs docs/help, crate::help::{PAGES, IMAGES} and crate::help_icons::icon_exists"]
fn guide_is_consistent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let docs = load(&root.join("docs/help"));
    let mut report = check_guide(&docs, CONTRACT, &icon_exists);
    let pages: Vec<(&str, &str)> = PAGES.iter().map(|&(lang, id, _)| (lang, id)).collect();
    let stem = |n: &'static str| n.rsplit('/').next().unwrap_or(n).trim_end_matches(".webp");
    let images: Vec<&str> = IMAGES.iter().map(|&(name, _)| stem(name)).collect();
    report.problems.extend(check_embedded(&docs, &pages, &images));
    report.problems.extend(check_spots(&sources(root), &docs));
    for p in &report.pending {
        println!("image still to make: {p}");
    }
    let n = report.problems.len();
    assert!(n == 0, "the guide has {n} problems:\n{}", report.problems.join("\n"));
}

mod tests;
