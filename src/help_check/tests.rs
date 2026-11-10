//! The checks of the guide on a small fake guide: it passes as it is, and each mistake
//! put into it is found, with its file and line.

use super::*;

/// A fake contract: two areas, three pages.
const FAKE: &[Area] = &[
    Area { names: ["Basics", "Base"], ids: &["one", "two"] },
    Area { names: ["More", "Altro"], ids: &["three"] },
];

const EN: &[(&str, &str)] = &[
    ("index", "# Contents\n\n## Basics\n- [One](one.md)\n- [Two](two.md)\n\n## More\n- [Three](three.md)\n"),
    ("one", "# One\n\nSee [Two](two.md) and ![Save](icon://save).\n\n## Part\n\n![The grid](../img/grid.webp)\n"),
    ("two", "# Two\n\n## A\n\n### B\n\n<!-- shot: editor-crop | crop | the crop frame -->\n"),
    ("three", "# Three\n\nOnline: [site](https://example.org/x#y).\n\n```\n# not a heading [x](nowhere.md)\n```\n"),
];

const IT: &[(&str, &str)] = &[
    ("index", "# Sommario\n\n## Base\n- [Uno](one.md)\n- [Due](two.md)\n\n## Altro\n- [Tre](three.md)\n"),
    ("one", "# Uno\n\nVedi [Due](two.md) e ![Salva](icon://save).\n\n## Parte\n\n![La griglia](../img/grid.webp)\n"),
    ("two", "# Due\n\n## A\n\n### B\n\n<!-- shot: editor-crop | ritaglio | il riquadro -->\n"),
    ("three", "# Tre\n\nIn rete: [sito](https://example.org/x#y).\n"),
];

fn fake() -> Docs {
    let mut docs = Docs::default();
    for (lang, pages) in [("en", EN), ("it", IT)] {
        let pages = pages.iter().map(|&(id, md)| (id.to_string(), md.to_string())).collect();
        docs.pages.insert(lang.to_string(), pages);
    }
    docs.images.insert("grid".to_string());
    docs
}

fn set(docs: &mut Docs, lang: &str, id: &str, md: &str) {
    docs.pages.get_mut(lang).unwrap().insert(id.to_string(), md.to_string());
}

/// Replaces `from` with `to` in one page.
fn edit(docs: &mut Docs, lang: &str, id: &str, from: &str, to: &str) {
    let md = docs.pages[lang][id].replacen(from, to, 1);
    set(docs, lang, id, &md);
}

fn check(docs: &Docs) -> Report {
    check_guide(docs, FAKE, &|name| name != "nope")
}

/// Some problem contains `needle`.
fn finds(problems: &[String], needle: &str) {
    assert!(problems.iter().any(|p| p.contains(needle)), "no problem with {needle:?} in {problems:#?}");
}

#[test]
fn fake_guide_passes() {
    let report = check(&fake());
    assert!(report.problems.is_empty(), "{:#?}", report.problems);
    assert_eq!(report.pending, ["docs/help/en/two.md:7: editor-crop", "docs/help/it/two.md:7: editor-crop"]);
}

#[test]
fn page_set() {
    let mut docs = fake();
    docs.pages.get_mut("it").unwrap().remove("three");
    set(&mut docs, "en", "four", "# Four\n");
    let p = check(&docs).problems;
    finds(&p, "docs/help/it/three.md: missing");
    finds(&p, "docs/help/en/four.md: not a page of the contract");
    finds(&p, "docs/help/it/index.md:8: lists three.md, which does not exist");
    finds(&p, "docs/help/en/index.md: does not list four.md");
}

#[test]
fn headings_match() {
    let mut docs = fake();
    edit(&mut docs, "it", "two", "### B", "## B");
    finds(&check(&docs).problems, "docs/help/it/two.md:5: heading 3 is level 2 but level 3 in English (line 5)");
    let mut docs = fake();
    edit(&mut docs, "it", "two", "### B", "");
    finds(&check(&docs).problems, "docs/help/it/two.md: 2 headings, the English page has 3");
    let mut docs = fake();
    edit(&mut docs, "en", "three", "Online", "#### Deep\n");
    finds(&check(&docs).problems, "docs/help/en/three.md:3: heading deeper than `###`");
    let mut docs = fake();
    edit(&mut docs, "en", "three", "Online", "# Again\n");
    finds(&check(&docs).problems, "docs/help/en/three.md:3: a second `#` title");
    let mut docs = fake();
    edit(&mut docs, "it", "one", "# Uno\n\n", "");
    finds(&check(&docs).problems, "docs/help/it/one.md:1: the first line must be the title");
}

#[test]
fn index_titles_and_entries() {
    let mut docs = fake();
    edit(&mut docs, "it", "one", "# Uno", "# Primo");
    finds(&check(&docs).problems, "docs/help/it/index.md:4: \"Uno\" but one.md is titled \"Primo\"");
    let mut docs = fake();
    edit(&mut docs, "en", "index", "- [Three](three.md)", "- [One](one.md)");
    let p = check(&docs).problems;
    finds(&p, "docs/help/en/index.md: lists one.md 2 times");
    finds(&p, "docs/help/en/index.md: does not list three.md");
    finds(&p, "docs/help/en/index.md: areas and pages differ from the contract");
    let mut docs = fake();
    edit(&mut docs, "it", "index", "## Altro", "## Riferimento");
    finds(&check(&docs).problems, "docs/help/it/index.md: areas and pages differ from the contract");
    let mut docs = fake();
    edit(&mut docs, "en", "index", "## More", "Some words\n## More");
    finds(&check(&docs).problems, "docs/help/en/index.md:7: unexpected line");
    let mut docs = fake();
    edit(&mut docs, "it", "index", "# Sommario", "# Indice");
    finds(&check(&docs).problems, "docs/help/it/index.md:1: the first line must be `# Sommario`");
}

#[test]
fn links() {
    let mut docs = fake();
    edit(&mut docs, "en", "one", "(two.md)", "(four.md)");
    finds(&check(&docs).problems, "docs/help/en/one.md:3: link to a missing page, four.md");
    let mut docs = fake();
    edit(&mut docs, "it", "one", "(two.md)", "(two.md#a)");
    finds(&check(&docs).problems, "docs/help/it/one.md:3: link (two.md#a): anchors are not supported");
    assert_eq!(classify("viewer.md"), Target::Page("viewer"));
    assert_eq!(classify("https://github.com/IoSonoPiero/eyecansee"), Target::External);
    for bad in ["../img/grid.png", "file:///tmp/a.md", "img/grid.webp", "../en/one.md", ".md", "mailto:a@b.c"] {
        assert!(matches!(classify(bad), Target::Bad(_)), "{bad}");
    }
}

#[test]
fn images() {
    let mut docs = fake();
    docs.images.remove("grid");
    finds(&check(&docs).problems, "docs/help/en/one.md:7: image grid.webp is not in docs/help/img");
    let mut docs = fake();
    docs.images.insert("lonely".to_string());
    finds(&check(&docs).problems, "docs/help/img/lonely.webp: not shown by any page");
    let mut docs = fake();
    docs.images.insert("other".to_string());
    edit(&mut docs, "it", "one", "../img/grid.webp", "../img/other.webp");
    finds(&check(&docs).problems, "docs/help/it/one.md: shows the images [\"other\"], the English page [\"grid\"]");
}

#[test]
fn icons() {
    let mut docs = fake();
    edit(&mut docs, "it", "one", "icon://save", "icon://nope");
    finds(&check(&docs).problems, "docs/help/it/one.md:3: unknown icon icon://nope");
}

#[test]
fn placeholders() {
    let mut docs = fake();
    docs.images.insert("editor-crop".to_string());
    finds(&check(&docs).problems, "docs/help/en/two.md:7: placeholder for editor-crop.webp, which exists");
    let mut docs = fake();
    edit(&mut docs, "it", "two", "<!-- shot: editor-crop", "<!-- shot: crop");
    finds(&check(&docs).problems, "docs/help/it/two.md: has the placeholders [\"crop\"], the English page [\"editor-crop\"]");
}

#[test]
fn f1_spots() {
    let src = "let a = \"help:one\";\nlet b = (\"help:four\", \"help:\", \"help:Bad id\");\n";
    assert_eq!(spots(src), [(1, "one"), (2, "four")]);
    let p = check_spots(&[("src/a.rs".to_string(), src.to_string())], &fake());
    assert_eq!(
        p,
        ["src/a.rs:2: \"help:four\" has no docs/help/en/four.md", "src/a.rs:2: \"help:four\" has no docs/help/it/four.md"]
    );
}

#[test]
fn embedded() {
    let docs = fake();
    let mut pages: Vec<(&str, &str)> =
        docs.pages.iter().flat_map(|(l, p)| p.keys().map(move |id| (l.as_str(), id.as_str()))).collect();
    assert!(check_embedded(&docs, &pages, &["grid"]).is_empty());
    pages.retain(|&p| p != ("it", "three"));
    pages.push(("en", "zzz"));
    let p = check_embedded(&docs, &pages, &["pic"]);
    finds(&p, "docs/help/it/three.md: not embedded in help::PAGES");
    finds(&p, "help::PAGES has en/zzz, but docs/help/en/zzz.md does not exist");
    finds(&p, "help::IMAGES has pic, but docs/help/img/pic.webp does not exist");
    finds(&p, "docs/help/img/grid.webp: not embedded in help::IMAGES");
}

#[test]
fn load_from_disk() {
    let root = std::env::temp_dir().join(format!("eyecansee-help-check-{}", std::process::id()));
    for (path, text) in [("en/one.md", "# One\n"), ("en/notes.txt", ""), ("it/one.md", "# Uno\n"), ("img/grid.webp", "")] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    for name in ["grid.png", "CREDITS.md", "notes.md"] {
        std::fs::write(root.join("img").join(name), "").unwrap();
    }
    let docs = load(&root);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(docs.pages["en"]["one"], "# One\n");
    assert_eq!(docs.pages["it"].keys().collect::<Vec<_>>(), ["one"]);
    assert_eq!(docs.images.iter().collect::<Vec<_>>(), ["grid"]);
    finds(&docs.problems, "notes.txt: not a .md page");
    finds(&docs.problems, "grid.png: images are .webp");
    finds(&docs.problems, "notes.md: images are .webp");
    assert!(!docs.problems.iter().any(|p| p.contains("CREDITS.md")), "{:?}", docs.problems);
    assert_eq!(load(&root).problems.len(), 3, "a missing folder for each language and for the images");
}
