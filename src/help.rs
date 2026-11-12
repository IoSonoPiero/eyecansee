//! The guide (F1, Help > Guide): pages embedded in the executable (`docs/help/<lang>/<id>.md`,
//! pictures in `docs/help/img/`), a table of contents read from `index.md`, Back and Forward as in
//! a browser, and a search over every page. The search itself is in `help_search`, without egui.

use eframe::egui::{self, text::LayoutJob};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer, SearchOptions};

use crate::help_search::{self, Area, Hit, Kind};
use crate::{App, icons};

/// The pages, listed once: each id has its file in every language.
macro_rules! pages {
    ($($id:literal),* $(,)?) => {
        /// (language, id, text) of every page, the table of contents (`index`) included.
        pub(crate) const PAGES: &[(&str, &str, &str)] = &[
            $(("en", $id, include_str!(concat!("../docs/help/en/", $id, ".md"))),)*
            $(("it", $id, include_str!(concat!("../docs/help/it/", $id, ".md"))),)*
        ];
    };
}

pages![
    "index", "start", "grid", "folders", "address", "bookmarks", "selection", "viewer", "metadata", "slideshow", "video",
    "model-3d", "model-export", "cad", "editor", "editor-marquee", "editor-lines", "editor-draw", "editor-text", "editor-lens",
    "editor-stamps", "editor-cover", "editor-filters", "editor-crop", "editor-colors", "editor-save", "rotate", "save-as",
    "convert", "convert-video", "copy-move", "rename", "delete", "new-image", "duplicates", "devices", "phone", "servers",
    "server-manager", "passwords", "options", "updates", "components", "keyboard", "formats", "limits",
];

/// The pictures of the pages, the same for every language.
macro_rules! images {
    ($($name:literal),* $(,)?) => {
        /// (name, bytes) of every `docs/help/img/<name>.webp`.
        pub(crate) const IMAGES: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!("../docs/help/img/", $name, ".webp"))),)*
        ];
    };
}

images![
    "trash-strip",
    "phone",
    "bookmarks-menu",
    "cad",
    "convert",
    "convert-video",
    "devices",
    "duplicates",
    "editor",
    "editor-crop",
    "editor-props",
    "metadata-panel",
    "model-3d",
    "new-image",
    "open-location",
    "options",
    "save-as",
    "server-manager",
    "side-panel",
    "start",
    "video",
    "viewer",
];

/// The address of a picture as the pages write it (`../img/<name>.webp`) with the prefix the
/// viewer gives to addresses without a scheme.
fn image_uri(name: &str) -> String {
    format!("bytes://help/../img/{name}.webp")
}

/// The id of a page from a point of F1 (`"help:<id>"`) or from an id.
fn page_id(spot: &str) -> Option<&'static str> {
    let id = spot.strip_prefix("help:").unwrap_or(spot);
    PAGES.iter().find(|p| p.1 == id).map(|p| p.1)
}

/// The language of the pages: the one of the interface if it has them, otherwise English.
fn pages_lang(code: &str) -> &'static str {
    PAGES.iter().find(|p| p.0 == code).map_or("en", |p| p.0)
}

/// The text of a page in a language.
fn page_text(lang: &str, id: &str) -> &'static str {
    PAGES.iter().find(|p| p.0 == lang && p.1 == id).map_or("", |p| p.2)
}

/// The pages of a language, (id, text), without the table of contents: what the search reads.
fn searched(lang: &str) -> Vec<(&'static str, &'static str)> {
    PAGES.iter().filter(|p| p.0 == lang && p.1 != "index").map(|p| (p.1, p.2)).collect()
}

/// The id of the page viewer, for egui_commonmark's search in the page.
fn viewer_id() -> egui::Id {
    egui::Id::unique("help-page")
}

/// The id of the guide's window.
fn window_id() -> egui::Id {
    egui::Id::unique("help-window")
}

/// Two presses of Esc within this many seconds close the guide.
const DOUBLE_ESC: f64 = 0.5;

/// The guide's window: open or not, the page shown, the history and the search.
pub(crate) struct Guide {
    pub(crate) open: bool,
    lang: &'static str,
    page: &'static str,
    back: Vec<&'static str>,
    forward: Vec<&'static str>,
    /// The viewer's state for the page shown: made again at every page.
    cache: CommonMarkCache,
    query: String,
    hits: Vec<Hit>,
    /// The pictures are given to egui once, the first time the guide opens.
    images: bool,
    /// After a result is opened, the match is brought into view again until then (time of
    /// egui): the pictures above it are decoded in the background and push it down.
    settle: Option<f64>,
    /// When the last Esc for the guide was pressed (time of egui), for the double Esc.
    last_esc: Option<f64>,
    /// The search field had the keyboard in the frame before: an Esc there is the guide's.
    typing: bool,
    /// The search field takes the keyboard at the next frame: the guide has just opened.
    focus: bool,
}

impl Default for Guide {
    fn default() -> Self {
        Self {
            open: false,
            lang: "en",
            page: "index",
            back: vec![],
            forward: vec![],
            cache: fresh_cache(),
            query: String::new(),
            hits: vec![],
            images: false,
            settle: None,
            last_esc: None,
            typing: false,
            focus: false,
        }
    }
}

/// A new viewer state that reports the clicks on the links to the pages instead of opening them.
fn fresh_cache() -> CommonMarkCache {
    let mut cache = CommonMarkCache::default();
    for p in PAGES.iter().filter(|p| p.0 == "en") {
        cache.add_link_hook(format!("{}.md", p.1));
    }
    cache
}

impl Guide {
    /// Another page; the one left goes in the history.
    fn go(&mut self, id: &'static str) {
        if id != self.page {
            self.back.push(self.page);
            self.forward.clear();
            self.page = id;
        }
        self.fresh();
    }

    fn back(&mut self) {
        if let Some(id) = self.back.pop() {
            self.forward.push(self.page);
            self.page = id;
            self.fresh();
        }
    }

    fn forward(&mut self) {
        if let Some(id) = self.forward.pop() {
            self.back.push(self.page);
            self.page = id;
            self.fresh();
        }
    }

    /// The interface changed language: the pages of the new one, the search done again.
    pub(crate) fn language(&mut self, code: &str) {
        self.lang = pages_lang(code);
        self.cache = fresh_cache();
        self.search();
    }

    /// The results of the field, and its words highlighted in the page shown.
    fn search(&mut self) {
        self.hits = help_search::search(&searched(self.lang), &self.query);
        self.highlight();
    }

    /// The page shown from the start, with the words of the field highlighted.
    fn fresh(&mut self) {
        self.cache = fresh_cache();
        self.highlight();
    }

    /// The matches highlighted in the page follow the search field: none when it is empty, only
    /// those of the new words when it changes. The page does not move for it while typing.
    fn highlight(&mut self) {
        let id = viewer_id();
        *self.cache.search_options_mut(&id) = SearchOptions::REGEX;
        *self.cache.search_query_mut(&id) = help_search::pattern(&self.query);
        self.cache.update_search_matches(&id, page_text(self.lang, self.page));
        let found = self.cache.search_cache_mut(&id);
        if found.search_ranges.is_empty() {
            // With no words egui_commonmark keeps the old active match: dropped here.
            found.active_match = None;
            found.sync_active_search_range();
        }
        found.take_pending_scroll_to_active_match();
        self.settle = None;
    }

    /// An Esc for the guide at `now` (seconds): true when it is the second within `DOUBLE_ESC`.
    fn esc(&mut self, now: f64) -> bool {
        let double = self.last_esc.is_some_and(|t| now - t <= DOUBLE_ESC);
        self.last_esc = if double { None } else { Some(now) };
        double
    }

    /// Opens the page of a result with every match of the search highlighted, the one of the
    /// result active and in view.
    fn open_hit(&mut self, i: usize) {
        let Some(hit) = self.hits.get(i).cloned() else { return };
        self.go(hit.id);
        let id = viewer_id();
        let target = self.cache.search_ranges(&id).iter().position(|r| r.end > hit.at).unwrap_or(0);
        let now = self.cache.active_match(&id).unwrap_or(0);
        self.cache.go_to_match(&id, target.cast_signed() - now.cast_signed());
        self.settle = Some(f64::NEG_INFINITY);
    }

    /// Keeps the match of the result opened in view for a second, unless the user scrolls.
    fn keep_match_in_view(&mut self, ctx: &egui::Context) {
        let Some(until) = self.settle else { return };
        let (now, scrolled) = ctx.input(|i| (i.time, i.smooth_scroll_delta != egui::Vec2::ZERO || i.pointer.any_down()));
        if until == f64::NEG_INFINITY {
            self.settle = Some(now + 1.0);
        } else if now > until || scrolled {
            self.settle = None;
            return;
        }
        self.cache.go_to_match(&viewer_id(), 0);
        ctx.request_repaint();
    }

    /// The link to a page clicked in the page shown, if any.
    fn clicked_link(&mut self) -> Option<&'static str> {
        let target = self.cache.link_hooks().iter().find(|&(_, &clicked)| clicked).map(|(k, _)| k.clone())?;
        self.cache.link_hooks_mut().insert(target.clone(), false);
        page_id(target.strip_suffix(".md")?)
    }

    fn act(&mut self, todo: Do) {
        match todo {
            Do::Go(id) => self.go(id),
            Do::Back => self.back(),
            Do::Forward => self.forward(),
            Do::Search => self.search(),
            Do::Hit(i) => self.open_hit(i),
            Do::Match(step) => self.cache.go_to_match(&viewer_id(), step),
        }
    }
}

/// What the window asks, done once it is drawn.
enum Do {
    Go(&'static str),
    Back,
    Forward,
    Search,
    Hit(usize),
    Match(isize),
}

/// What the window shows, prepared before drawing it.
struct View {
    title: String,
    text: &'static str,
    areas: Vec<Area>,
    /// The results with the title of their page.
    hits: Vec<(String, Hit)>,
    /// "3 of 12" when the page has matches of the search highlighted.
    matches: Option<String>,
    can_back: bool,
    can_forward: bool,
    tips: [String; 5],
    hint: String,
    clear: String,
    nothing: String,
}

impl App {
    /// F1's entry: `"help:<id>"` or `"<id>"`. The guide open on that page closes; otherwise it
    /// opens there. An unknown page opens the table of contents.
    pub(crate) fn toggle_help(&mut self, spot: &str) {
        let id = page_id(spot).unwrap_or_else(|| {
            log!("help: no page for {spot:?}");
            "index"
        });
        if self.help.open && self.help.page == id {
            self.help.open = false;
        } else {
            self.open_help(id);
        }
    }

    /// Opens the guide on a page (Help > Guide: the table of contents).
    pub(crate) fn open_help(&mut self, id: &'static str) {
        self.help.open = true;
        self.help.typing = false;
        self.help.focus = true;
        self.help.go(id);
    }

    /// Before drawing: an Esc over the guide (the pointer on it, or typing in its search field)
    /// is the guide's and reaches nothing under it (viewer, editor, slideshow). Once it only
    /// leaves the field (egui does it); twice within `DOUBLE_ESC` it closes the guide. The
    /// questions are not reached: their Esc is taken before (`raw_input_hook`).
    pub(crate) fn help_esc(&mut self, ctx: &egui::Context) {
        if !self.help.open {
            return;
        }
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let over = pointer.zip(ctx.memory(|m| m.area_rect(window_id()))).is_some_and(|(p, r)| r.contains(p));
        if !over && !self.help.typing {
            return;
        }
        let (esc, now) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::Escape), i.time));
        if esc && self.help.esc(now) {
            log!("help: closed with a double Esc");
            self.help.open = false;
            self.help.typing = false;
        }
    }

    /// The guide's window, when open.
    pub(crate) fn help_window(&mut self, ctx: &egui::Context) {
        #[cfg(debug_assertions)]
        self.help_debug_start();
        if !self.help.open {
            return;
        }
        if !self.help.images {
            for (name, bytes) in IMAGES {
                ctx.include_bytes(image_uri(name), *bytes);
            }
            self.help.images = true;
        }
        if self.help.lang != pages_lang(self.i18n.code) {
            self.help.language(self.i18n.code);
        }
        self.help.keep_match_in_view(ctx);
        // F1 works with a dialog open: the guide goes over it and its backdrop, and stays usable.
        let id = window_id();
        let layer = egui::LayerId::new(egui::Order::Foreground, id);
        if !ctx.memory(|m| m.allows_interaction(layer)) {
            ctx.move_to_top(layer);
        }
        let view = self.help_view();
        let mut open = true;
        let todo = egui::Window::new(view.title.clone())
            .id(id)
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_size([900.0, 620.0])
            .min_size([480.0, 320.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| draw(ui, &mut self.help, &view))
            .and_then(|r| r.inner.flatten());
        self.help.open &= open;
        if let Some(todo) = todo {
            self.help.act(todo);
        }
    }

    fn help_view(&self) -> View {
        let g = &self.help;
        let t = |id| self.i18n.t(id);
        let title_of = |id| help_search::title(page_text(g.lang, id)).to_string();
        let id = viewer_id();
        let total = g.cache.search_ranges(&id).len();
        let matches = (total > 0).then(|| {
            let n = g.cache.active_match(&id).map_or(0, |i| i + 1).to_string();
            self.i18n.t_args("counter", &[("n", &n), ("total", &total.to_string())])
        });
        View {
            title: t("menu-manual"),
            text: page_text(g.lang, g.page),
            areas: help_search::contents(page_text(g.lang, "index")),
            hits: g.hits.iter().map(|h| (title_of(h.id), h.clone())).collect(),
            matches,
            can_back: !g.back.is_empty(),
            can_forward: !g.forward.is_empty(),
            tips: ["help-back", "help-forward", "help-contents", "help-previous-match", "help-next-match"].map(t),
            hint: t("help-search"),
            clear: t("help-clear-search"),
            nothing: t("help-nothing-found"),
        }
    }

    /// Debug builds only: `EYECANSEE_HELP=<id>` opens the guide on a page at the start,
    /// `EYECANSEE_HELP=?<words>` searches them and opens the first result (`?<words>#<n>` the
    /// result n), to check the window
    /// with `EYECANSEE_SCREENSHOT`.
    #[cfg(debug_assertions)]
    fn help_debug_start(&mut self) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static DONE: AtomicBool = AtomicBool::new(false);
        let Ok(start) = std::env::var("EYECANSEE_HELP") else { return };
        if DONE.swap(true, Ordering::Relaxed) {
            return;
        }
        self.help.language(self.i18n.code);
        match start.strip_prefix('?') {
            Some(words) => {
                // `?<words>#<n>`: the result n (from 1) instead of the first.
                let (words, n) = words.rsplit_once('#').map_or((words, 1), |(w, n)| (w, n.parse().unwrap_or(1)));
                self.open_help("index");
                self.help.query = words.to_string();
                self.help.search();
                self.help.open_hit(n.max(1) - 1);
            }
            None => self.toggle_help(&start),
        }
    }
}

/// Draws the window: the bar, the contents or the results on the left, the page on the right.
fn draw(ui: &mut egui::Ui, g: &mut Guide, v: &View) -> Option<Do> {
    let mut todo = None;
    egui::Panel::top("help-bar").show(ui, |ui| bar(ui, v, &mut todo));
    egui::Panel::left("help-side").resizable(true).default_size(260.0).size_range(160.0..=480.0).show(ui, |ui| side(ui, g, v, &mut todo));
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt(("help-page", g.page)).auto_shrink(false).show(ui, |ui| {
            CommonMarkViewer::new().default_implicit_uri_scheme("bytes://help/").show_with_id(viewer_id(), ui, &mut g.cache, v.text);
        });
    });
    if let Some(id) = g.clicked_link() {
        todo = Some(Do::Go(id));
    }
    todo
}

fn bar(ui: &mut egui::Ui, v: &View, todo: &mut Option<Do>) {
    ui.horizontal(|ui| {
        let [back, forward, contents, previous, next] = &v.tips;
        if ui.add_enabled(v.can_back, icons::button(ui, icons::Line::Back)).on_hover_text(back).clicked() {
            *todo = Some(Do::Back);
        }
        if ui.add_enabled(v.can_forward, icons::button(ui, icons::Line::Forward)).on_hover_text(forward).clicked() {
            *todo = Some(Do::Forward);
        }
        if ui.add(icons::button(ui, icons::Line::Manual)).on_hover_text(contents).clicked() {
            *todo = Some(Do::Go("index"));
        }
        if let Some(matches) = &v.matches {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(icons::button(ui, icons::Line::Down)).on_hover_text(next).clicked() {
                    *todo = Some(Do::Match(1));
                }
                if ui.add(icons::button(ui, icons::Line::ChevronUp)).on_hover_text(previous).clicked() {
                    *todo = Some(Do::Match(-1));
                }
                ui.label(matches);
            });
        }
    });
}

/// The search field and under it the results, or the table of contents when it is empty.
fn side(ui: &mut egui::Ui, g: &mut Guide, v: &View, todo: &mut Option<Do>) {
    ui.add_space(4.0);
    search_field(ui, g, v, todo);
    ui.add_space(4.0);
    egui::ScrollArea::vertical().id_salt("help-side").auto_shrink(false).show(ui, |ui| {
        if g.query.trim().is_empty() {
            for area in &v.areas {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(&area.name).strong());
                for (title, id) in &area.pages {
                    if ui.selectable_label(g.page == id.as_str(), title).clicked()
                        && let Some(id) = page_id(id)
                    {
                        *todo = Some(Do::Go(id));
                    }
                }
            }
        } else if v.hits.is_empty() {
            ui.weak(&v.nothing);
        } else {
            for (i, (title, hit)) in v.hits.iter().enumerate() {
                let job = hit_text(ui, title, hit);
                if ui.add(egui::Button::selectable(false, job).wrap()).clicked() {
                    *todo = Some(Do::Hit(i));
                }
                ui.add_space(4.0);
            }
        }
    });
}

/// The search field with a ✕ on its right that empties it, and the highlight in the page with it.
fn search_field(ui: &mut egui::Ui, g: &mut Guide, v: &View, todo: &mut Option<Do>) {
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let clear = ui.add_enabled(!g.query.is_empty(), icons::small(ui, icons::Line::Close)).on_hover_text(&v.clear);
        if clear.clicked() {
            g.query.clear();
            *todo = Some(Do::Search);
        }
        let field = ui.add(egui::TextEdit::singleline(&mut g.query).hint_text(&v.hint).desired_width(f32::INFINITY));
        if std::mem::take(&mut g.focus) {
            field.request_focus();
        }
        g.typing = field.has_focus();
        if field.changed() {
            *todo = Some(Do::Search);
        }
    });
}

/// A result: the page's title, then the subtitle or the sentence found with the match in bold.
fn hit_text(ui: &egui::Ui, title: &str, hit: &Hit) -> LayoutJob {
    let style = ui.style();
    let font = egui::TextStyle::Body.resolve(style);
    let (strong, weak, normal) = (ui.visuals().strong_text_color(), ui.visuals().weak_text_color(), ui.visuals().text_color());
    let format = |font: &egui::FontId, color| egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let (lead, text_font, text_color) = match hit.kind {
        Kind::Title => (None, &font, normal),
        Kind::Heading => (Some(format!("{title} › ")), &font, normal),
        Kind::Text => (Some(format!("{title}\n")), &font, weak),
    };
    if let Some(lead) = lead {
        job.append(&lead, 0.0, format(&font, normal));
    }
    let e = &hit.excerpt;
    job.append(&e[..hit.mark.start], 0.0, format(text_font, text_color));
    job.append(&e[hit.mark.clone()], 0.0, format(text_font, strong));
    job.append(&e[hit.mark.end..], 0.0, format(text_font, text_color));
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id has its page in every language, and a point of F1 finds its page.
    #[test]
    fn pages_and_points() {
        let ids = |lang| PAGES.iter().filter(|p| p.0 == lang).map(|p| p.1).collect::<Vec<_>>();
        assert_eq!(ids("en"), ids("it"));
        assert_eq!(ids("en").len(), 46);
        assert_eq!(page_id("help:editor-crop"), Some("editor-crop"));
        assert_eq!(page_id("grid"), Some("grid"));
        // Built at run time: a "help:…" literal must name a real page (help_check).
        assert_eq!(page_id(&format!("help:{}", "nothing")), None);
        assert_eq!(pages_lang("it"), "it");
        assert_eq!(pages_lang("xx"), "en");
        assert_eq!(help_search::title(page_text("it", "editor-crop")), "Ritagliare e ruotare");
        assert_eq!(image_uri("grid"), "bytes://help/../img/grid.webp");
        assert!(searched("en").iter().all(|p| p.0 != "index"));
    }

    /// Back and Forward as in a browser: a new page empties Forward.
    #[test]
    fn history() {
        let mut g = Guide::default();
        g.go("grid");
        g.go("viewer");
        g.back();
        assert_eq!((g.page, g.back.clone(), g.forward.clone()), ("grid", vec!["index"], vec!["viewer"]));
        g.forward();
        assert_eq!(g.page, "viewer");
        g.back();
        g.go("editor");
        assert_eq!((g.page, g.back.clone(), g.forward.is_empty()), ("editor", vec!["index", "grid"], true));
        g.go("editor");
        assert_eq!(g.back.len(), 2, "the same page is not added twice");
    }

    /// A result opens its page with the matches of the search highlighted.
    #[test]
    fn result_opens_highlighted() {
        let mut g = Guide::default();
        g.language("it");
        g.query = "RITAGLIARE".into();
        g.search();
        assert_eq!(g.hits.first().map(|h| h.id), Some("editor-crop"));
        g.open_hit(0);
        assert_eq!(g.page, "editor-crop");
        assert!(!g.cache.search_ranges(&viewer_id()).is_empty());
        assert_eq!(g.cache.active_match(&viewer_id()), Some(0));
    }

    /// The highlight follows the field: emptied, nothing; another word, only its matches; on
    /// another page, the words of the field.
    #[test]
    fn highlight_follows_the_field() {
        let ranges = |g: &Guide| g.cache.search_ranges(&viewer_id()).to_vec();
        let only = |g: &Guide, word: &str| {
            let text = page_text(g.lang, g.page);
            let found = ranges(g);
            !found.is_empty() && found.iter().all(|r| text[r.clone()].to_lowercase() == word)
        };
        let mut g = Guide::default();
        g.language("en");
        g.query = "crop".into();
        g.search();
        g.open_hit(0);
        assert!(only(&g, "crop"));
        g.query.clear();
        g.search();
        assert!(ranges(&g).is_empty());
        assert_eq!(g.cache.active_match(&viewer_id()), None);
        g.query = "rotate".into();
        g.search();
        assert!(only(&g, "rotate"), "only the new word");
        g.go("rotate");
        assert!(only(&g, "rotate"), "another page, the words of the field");
        g.query = "   ".into();
        g.search();
        assert!(ranges(&g).is_empty(), "spaces are no search");
    }

    /// Two presses of Esc close the guide only when close together; a third starts again.
    #[test]
    fn double_esc() {
        let mut g = Guide::default();
        assert!(!g.esc(10.0));
        assert!(g.esc(10.4));
        assert!(!g.esc(10.5), "a third press starts again");
        assert!(!g.esc(11.2), "too late");
        assert!(g.esc(11.6));
    }
}
