//! The guide (F1, Help > Guide): pages embedded in the executable (`docs/help/<lang>/<id>.md`,
//! pictures in `docs/help/img/`), a table of contents read from `index.md`, Back and Forward as in
//! a browser, and a search over every page. The search itself is in `help_search`, without egui.
//!
//! The pictures (issue #280): the pages of every language name the same ones, `../img/<name>.webp`.
//! Where a picture shows words the page names, a language has its own in `docs/help/img/<lang>/`;
//! the others, and a language without its own, show the one in `docs/help/img/`, English or
//! without words.

use eframe::egui::{self, text::LayoutJob};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer, SearchOptions};

use crate::help_search::{self, Area, Hit, Kind};
use crate::{App, icons};
use crate::confirm::FollowArea;

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

/// The pictures of the pages: first those in English or without words, then for each language
/// its own.
macro_rules! images {
    ([$($name:literal),* $(,)?] $(, $lang:literal: [$($own:literal),* $(,)?])* $(,)?) => {
        /// (language, name, bytes) of every `docs/help/img/<name>.webp` ("en") and
        /// `docs/help/img/<language>/<name>.webp`.
        pub(crate) const IMAGES: &[Picture] = &[
            $(("en", $name, include_bytes!(concat!("../docs/help/img/", $name, ".webp"))),)*
            $($(($lang, $own, include_bytes!(concat!("../docs/help/img/", $lang, "/", $own, ".webp"))),)*)*
        ];
    };
}

images![
    [
        "trash-strip", "phone", "bookmarks-menu", "cad", "convert", "convert-video", "devices", "duplicates", "editor", "editor-crop",
        "editor-props", "metadata-panel", "model-3d", "new-image", "open-location", "options", "save-as", "server-manager",
        "side-panel", "start", "tips", "video", "viewer", "viewer-360",
    ],
    "it": [
        "trash-strip", "phone", "bookmarks-menu", "cad", "convert", "convert-video", "devices", "duplicates", "editor", "editor-crop",
        "editor-props", "metadata-panel", "model-3d", "new-image", "open-location", "options", "save-as", "server-manager",
        "side-panel", "start", "tips", "video", "viewer", "viewer-360",
    ],
];

/// A picture built into the program: its language, its name and its bytes. "en" is also the
/// language of the pictures without words, the same for everyone.
pub(crate) type Picture = (&'static str, &'static str, &'static [u8]);

/// The picture `name` for a language: its own if it has one, otherwise the English one.
pub(crate) fn picture(pictures: &[Picture], lang: &str, name: &str) -> Option<&'static [u8]> {
    let of = |lang: &str| pictures.iter().find(|p| p.0 == lang && p.1 == name).map(|p| p.2);
    of(lang).or_else(|| of("en"))
}

/// The prefix the viewer gives to the addresses without a scheme of the pages of a language:
/// each language has its own, so the same `../img/<name>.webp` leads to its picture.
fn pages_uri(lang: &str) -> String {
    format!("bytes://help/{lang}/")
}

/// The address of a picture as the pages write it (`../img/<name>.webp`), in a language.
fn image_uri(lang: &str, name: &str) -> String {
    format!("{}../img/{name}.webp", pages_uri(lang))
}

/// The pictures given to egui, once: for the pages of each language its own, or the English ones.
fn load_images(ctx: &egui::Context) {
    let done = egui::Id::unique("help-images");
    if !ctx.data(|d| d.get_temp::<bool>(done).unwrap_or(false)) {
        let mut langs: Vec<&str> = PAGES.iter().map(|p| p.0).collect();
        langs.dedup();
        for lang in langs {
            for name in IMAGES.iter().filter(|p| p.0 == "en").map(|p| p.1) {
                if let Some(bytes) = picture(IMAGES, lang, name) {
                    ctx.include_bytes(image_uri(lang, name), bytes);
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(done, true));
    }
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

/// With a window that asks open, what `raw_input_hook` leaves in the input instead of taking it
/// for that window: F1, and, while the guide's search field has the keyboard (`typing`), the text
/// and the keys that edit it, with Esc, which leaves it. The hook took them all, and nothing
/// could be searched with the guide over a message (#196). Not Enter (it stays an answer, which
/// the guide takes: `help_esc`), nor Tab (it would walk to the buttons of the window under), nor
/// the keys of the letters: the field needs their text only.
pub(crate) fn stays(typing: bool, event: &egui::Event) -> bool {
    use egui::Key;
    match event {
        egui::Event::Key { key: Key::F1, .. } => true,
        egui::Event::Text(_) => typing,
        egui::Event::Key { key, modifiers, .. } => {
            let edits = matches!(key, Key::Escape | Key::Backspace | Key::Delete | Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End);
            typing && (edits || modifiers.command && matches!(key, Key::A | Key::Z | Key::Y))
        }
        _ => false,
    }
}

/// A key or some text: what `held_back` keeps for the guide's search field.
fn is_typed(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Key { .. } | egui::Event::Text(_))
}

/// The keys and the text of this frame taken out of the input, for the guide's search field
/// alone with a window under the guide (`search_field` gives them to it while it is drawn):
/// nothing drawn before or after the field reads them, neither the page (the slideshow reads the
/// arrows whoever has the keyboard) nor the window under the guide (a tip does too).
fn held_back(input: &mut egui::InputState) -> Vec<egui::Event> {
    let (typed, rest) = std::mem::take(&mut input.events).into_iter().partition(is_typed);
    input.events = rest;
    input.keys_down.clear();
    typed
}

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
    /// After a result is opened, the match is brought into view again until then (time of
    /// egui): the pictures above it are decoded in the background and push it down.
    settle: Option<f64>,
    /// How far down the page shown is scrolled, in points, in the last frame.
    pub(crate) scrolled: f32,
    /// When the last Esc for the guide was pressed (time of egui), for the double Esc.
    last_esc: Option<f64>,
    /// The search field had the keyboard in the frame before: an Esc there is the guide's.
    typing: bool,
    /// A control of the guide (a link, the index) had the keyboard in the frame before (Tab):
    /// an Esc there is the guide's too, wherever the pointer is (#311, Piero's test L1.5).
    inside: bool,
    /// The search field takes the keyboard at the next frame: the guide has just opened.
    focus: bool,
    /// The keys and the text of this frame for the search field alone, with a window under the
    /// guide: out of the input (`held_back`) until the field is drawn.
    typed: Vec<egui::Event>,
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
            settle: None,
            scrolled: 0.0,
            last_esc: None,
            typing: false,
            inside: false,
            focus: false,
            typed: vec![],
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
    /// The guide is open and its search field had the keyboard in the frame before.
    pub(crate) fn in_field(&self) -> bool {
        self.open && self.typing
    }

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
        let text = page_text(self.lang, self.page);
        self.cache.update_search_matches(&id, text);
        let found = self.cache.search_cache_mut(&id);
        // What egui_commonmark finds in the description of a picture or an icon is not in the
        // page: not counted in "n of N", nor gone to.
        let active = found.active_match.and_then(|i| found.search_ranges.get(i)).map(|r| r.start);
        let pictures = help_search::images(text);
        found.search_ranges.retain(|r| !pictures.iter().any(|p| p.start <= r.start && r.end <= p.end));
        found.active_match = active.map(|a| found.search_ranges.iter().position(|r| r.start >= a).unwrap_or(0)).filter(|_| !found.search_ranges.is_empty());
        found.sync_active_search_range();
        if found.search_ranges.is_empty() {
            // With no words egui_commonmark keeps the old active match: dropped here.
            found.active_match = None;
            found.sync_active_search_range();
        }
        found.take_pending_scroll_to_active_match();
        self.settle = None;
    }

    /// An Esc for the guide at `now` (seconds): true when it closes the guide, the second within
    /// `DOUBLE_ESC` (one leaves the field). The same everywhere, over a window too
    /// (`App::under_guide`).
    fn esc(&mut self, now: f64) -> bool {
        let double = self.last_esc.is_some_and(|t| now - t <= DOUBLE_ESC);
        self.last_esc = if double { None } else { Some(now) };
        double
    }

    /// The page `id` at its section `title`, as a search for the title: the guide has no anchors,
    /// and the heading found is highlighted and in view. Whether the section is there.
    fn at(&mut self, id: &'static str, title: &str) -> bool {
        self.query = title.to_string();
        self.search();
        let Some(i) = self.hits.iter().position(|h| h.id == id && h.kind == help_search::Kind::Heading) else {
            self.go(id);
            return false;
        };
        self.open_hit(i);
        true
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

    /// Opens the guide on the page `id` at its section `title` (`Guide::at`).
    pub(crate) fn open_help_at(&mut self, id: &'static str, title: &str) {
        self.open_help(id);
        // The pages in the language of the interface before the title is looked for. The guide takes
        // it when its window is first drawn (`help_window`): opened here for the first time in a
        // language other than English, the title was looked for in the English pages and not found,
        // and the page stayed at its top (Piero, 26/11: "I have to scroll to find it").
        if self.help.lang != pages_lang(self.i18n.code) {
            self.help.language(self.i18n.code);
        }
        if !self.help.at(id, title) {
            log!("help: no section {title:?} in {id}");
        }
    }

    /// Opens the guide on a page (Help > Guide: the table of contents).
    pub(crate) fn open_help(&mut self, id: &'static str) {
        self.help.open = true;
        self.help.typing = false;
        self.help.inside = false;
        self.help.focus = true;
        self.help.go(id);
    }

    /// Before drawing: an Esc over the guide (the pointer on it, or typing in its search field)
    /// is the guide's and reaches nothing under it (viewer, editor, slideshow). Once it only
    /// leaves the field (egui does it); twice within `DOUBLE_ESC` it closes the guide. Over a
    /// window (`under_guide`) the guide is on top: wherever the pointer is, Esc is the guide's,
    /// and two close it there too; Enter and Esc taken for the window that asks (`key_answer`)
    /// are the guide's, so Enter answers nothing under it (it confirmed a deletion under the
    /// guide, #196), and so is Enter in the guide's field (Open location under it read it and
    /// opened). What is typed in the field over a window is held back for the field (`held_back`).
    pub(crate) fn help_esc(&mut self, ctx: &egui::Context) {
        self.help.typed.clear();
        if !self.help.open {
            return;
        }
        let under = self.under_guide(ctx);
        let answer = if under { self.key_answer.take() } else { None };
        // So are Tab, the arrows, Enter and Space for the buttons of that window (#311).
        if under {
            crate::confirm::drop_nav(ctx);
        }
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let over = pointer.zip(ctx.memory(|m| m.area_rect(window_id()))).is_some_and(|(p, r)| r.contains(p));
        if !over && !self.help.typing && !self.help.inside && !under {
            return;
        }
        let field = under && self.help.typing;
        let (esc, now) = ctx.input_mut(|i| {
            let esc = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            if field {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                self.help.typed = held_back(i);
            }
            (esc, i.time)
        });
        if (esc || answer == Some(false)) && self.help.esc(now) {
            log!("help: closed with Esc");
            self.help.open = false;
            self.help.typing = false;
            self.help.inside = false;
        }
    }

    /// A window that reads Esc is open under the guide, which goes over every one
    /// (`help_window`): a question or a message, another modal (renaming, a tip), Options or
    /// New image. Not the box of a copy under way: Esc is not its own.
    fn under_guide(&self, ctx: &egui::Context) -> bool {
        self.asking() || ctx.memory(|m| m.top_modal_layer()).is_some() || self.options || self.new_image.is_some()
    }

    /// The guide's window, when open.
    pub(crate) fn help_window(&mut self, ctx: &egui::Context) {
        #[cfg(debug_assertions)]
        self.help_debug_start();
        if !self.help.open {
            return;
        }
        load_images(ctx);
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
        let window = egui::Window::new(view.title.clone())
            .id(id)
            .order(egui::Order::Foreground)
            .open(&mut open)
            .default_size([900.0, 620.0])
            .min_size([480.0, 320.0])
            .resizable(true)
            .collapsible(false);
        // Tab with nothing focused goes into it, and stays inside it, not to the grid (#311).
        let todo = crate::app_dialogs::in_ring(ctx, layer, || window.show(ctx, |ui| draw(ui, &mut self.help, &view))).and_then(|r| r.inner.flatten());
        // Kept while nothing has the keyboard (the first Esc takes it away, the second must still
        // reach the guide); a click anywhere, or the keyboard elsewhere, ends it.
        match ctx.memory(|m| m.focused()) {
            Some(id) => self.help.inside = ctx.read_response(id).is_some_and(|r| r.layer_id == layer),
            None if ctx.input(|i| i.pointer.any_pressed()) => self.help.inside = false,
            None => {}
        }
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
        let page = egui::ScrollArea::vertical().id_salt(("help-page", g.page)).auto_shrink(false).show_follow(ui, |ui| {
            let top = ui.cursor().top();
            CommonMarkViewer::new().default_implicit_uri_scheme(pages_uri(g.lang)).show_with_id(viewer_id(), ui, &mut g.cache, v.text);
            top
        });
        g.scrolled = page.inner_rect.top() - page.inner;
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
    egui::ScrollArea::vertical().id_salt("help-side").auto_shrink(false).show_follow(ui, |ui| {
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
        // What was held back for the field (`held_back`): in the input only while it is drawn.
        let typed = std::mem::take(&mut g.typed);
        let held = !typed.is_empty();
        ui.input_mut(|i| i.events.extend(typed));
        let field = ui.add(egui::TextEdit::singleline(&mut g.query).hint_text(&v.hint).desired_width(f32::INFINITY));
        if held {
            ui.input_mut(|i| i.events.retain(|e| !is_typed(e)));
        }
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
        assert_eq!(image_uri("it", "grid"), "bytes://help/it/../img/grid.webp");
        assert!(searched("en").iter().all(|p| p.0 != "index"));
    }

    /// A picture in the language asked if it has its own, the English one otherwise, none for a
    /// name that is not there.
    #[test]
    fn picture_of_a_language_or_the_english_one() {
        let pictures: &[Picture] = &[("en", "menu", b"menu"), ("en", "photo", b"photo"), ("it", "menu", b"menu it")];
        assert_eq!(picture(pictures, "it", "menu"), Some(&b"menu it"[..]));
        assert_eq!(picture(pictures, "it", "photo"), Some(&b"photo"[..]), "no words in it: the same for every language");
        assert_eq!(picture(pictures, "en", "menu"), Some(&b"menu"[..]));
        assert_eq!(picture(pictures, "de", "menu"), Some(&b"menu"[..]), "a language without its own pictures");
        assert_eq!(picture(pictures, "it", "other"), None);
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

    /// "Why through the computer?" in the copy box and "Why do I have to wait?" in the window of
    /// the phone's own copy (#354) open the sections of the phone's page whose titles are
    /// `transfer-why-pc-section` and `phone-copy-why-section`, in every language: the headings are found.
    #[test]
    fn why_through_the_computer_has_its_section() {
        for (lang, _, _) in PAGES.iter().filter(|p| p.1 == "index") {
            for id in ["transfer-why-pc-section", "phone-copy-why-section"] {
                let title = crate::i18n::I18n::new(lang).t(id);
                assert!(page_text(lang, "phone").lines().any(|l| l == format!("## {title}")), "{lang}: no section {title:?} in phone.md");
                let mut g = Guide::default();
                g.language(lang);
                assert!(g.at("phone", &title), "{lang}: {title:?} not found");
                assert_eq!(g.page, "phone");
            }
        }
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

    /// The names of the icons are not counted among the matches of the page: on "Ritaglia e
    /// ruota" the icon's description is left out, the text after it stays.
    #[test]
    fn icons_not_counted() {
        let mut g = Guide::default();
        g.language("it");
        g.go("editor-crop");
        g.query = "ritaglia e ruota".into();
        g.search();
        let text = page_text("it", "editor-crop");
        let pictures = help_search::images(text);
        let ranges = g.cache.search_ranges(&viewer_id()).to_vec();
        assert!(!ranges.is_empty());
        assert!(ranges.iter().all(|r| !pictures.iter().any(|p| p.contains(&r.start))), "{ranges:?}");
        assert!(g.cache.active_match(&viewer_id()).is_some_and(|i| i < ranges.len()));
        let excerpt = &g.hits.iter().find(|h| h.id == "editor-crop" && h.kind == Kind::Text).unwrap().excerpt;
        assert!(!excerpt.contains("Ritaglia e ruota Ritaglia e ruota"), "{excerpt}");
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

    fn pressed(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    /// With a window that asks, F1 always stays in the input; the text and the keys that edit
    /// only while the guide's search field has the keyboard, and never Enter, Tab, a function
    /// key or the key of a letter.
    #[test]
    fn what_stays_for_the_search_field() {
        use egui::{Key, Modifiers as M};
        let text = egui::Event::Text("c".into());
        assert!(stays(false, &pressed(Key::F1, M::NONE)) && stays(true, &pressed(Key::F1, M::NONE)));
        assert!(stays(true, &text) && !stays(false, &text));
        for key in [Key::Backspace, Key::Delete, Key::ArrowLeft, Key::ArrowRight, Key::Home, Key::End, Key::Escape] {
            assert!(stays(true, &pressed(key, M::NONE)) && stays(true, &pressed(key, M::SHIFT)), "{key:?} in the field");
            assert!(!stays(false, &pressed(key, M::NONE)), "{key:?} out of the field");
        }
        for key in [Key::A, Key::Z, Key::Y] {
            assert!(stays(true, &pressed(key, M::COMMAND)) && !stays(true, &pressed(key, M::NONE)) && !stays(false, &pressed(key, M::COMMAND)), "{key:?}");
        }
        for key in [Key::Enter, Key::Tab, Key::Space, Key::F5, Key::F11, Key::C] {
            assert!(!stays(true, &pressed(key, M::NONE)), "{key:?}");
        }
        assert!(!stays(true, &egui::Event::Copy), "what is not a key is not this function's");
    }

    /// What is held back for the field leaves the input, keys held included; the rest stays.
    #[test]
    fn keys_held_back_leave_the_input() {
        let mut i = egui::InputState::default();
        i.events = vec![pressed(egui::Key::ArrowRight, egui::Modifiers::NONE), egui::Event::Text("a".into()), egui::Event::Copy, egui::Event::PointerGone];
        i.keys_down.insert(egui::Key::ArrowRight);
        let typed = held_back(&mut i);
        assert_eq!(typed.len(), 2);
        assert!(typed.iter().all(is_typed));
        assert_eq!(i.events, vec![egui::Event::Copy, egui::Event::PointerGone]);
        assert!(!i.key_pressed(egui::Key::ArrowRight) && !i.key_down(egui::Key::ArrowRight));
    }

    use crate::audit_family_1::{Harness, folder, own_folder};

    /// F1 from the window `ask` opened on the folder `name` (with a.jpg and b.jpg): the guide
    /// opens on the page of that window and sits over it.
    fn guide_over(name: &str, ask: impl FnOnce(&mut App)) -> (std::path::PathBuf, Harness) {
        let dir = folder(name, &["a.jpg", "b.jpg"]);
        let mut h = Harness::new(&dir);
        ask(&mut h.app);
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        h.key(egui::Key::F1);
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        let layer = egui::LayerId::new(egui::Order::Foreground, window_id());
        assert!(h.app.help.open && h.ctx.memory(|m| m.is_above_modal_layer(layer)), "the guide open over the window");
        (dir, h)
    }

    /// `text` typed as the window gives it: for every letter its key pressed, its text, its key
    /// released.
    fn type_text(h: &mut Harness, text: &str) {
        for c in text.chars() {
            let key = egui::Key::from_name(&c.to_uppercase().to_string()).unwrap();
            let ev = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
            h.frame(vec![ev(true), egui::Event::Text(c.to_string()), ev(false)]);
        }
    }

    /// One Esc: the guide stays open. A second at once: closed.
    fn esc_twice(h: &mut Harness) {
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.help.open, "one Esc closed the guide");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(!h.app.help.open, "two Esc left the guide open");
    }

    /// The message at the end of a copy, which names its page of the guide.
    fn copy_ended(app: &mut App) {
        let notice = crate::save_dialogs::Notice::new("Copy finished".into(), "Some links left out.".into());
        app.notice = Some(crate::save_dialogs::Notice { help: Some("help:copy-move"), ..notice });
    }

    /// R10 of 2026-11-21: with the guide opened by F1 over the message at the end of a copy,
    /// Esc closed the message under it and left the guide open. The guide is the window on top:
    /// Esc is its own, two close it as everywhere, and the next Esc closes the message (#196).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn esc_closes_the_guide_over_a_message() {
        if own_folder("help::tests::esc_closes_the_guide_over_a_message") {
            return;
        }
        let (_, mut h) = guide_over("guide-message", copy_ended);
        assert_eq!(h.app.help.page, "copy-move", "F1 opens the page the message names");
        h.key(egui::Key::F1);
        h.frame(Vec::new());
        assert!(!h.app.help.open && h.app.notice.is_some(), "F1 again closes the guide only");
        h.key(egui::Key::F1);
        h.frame(Vec::new());
        assert!(h.app.help.open, "and opens it again");
        // Two Esc too far apart: neither closes the guide, neither reaches the message.
        for _ in 0..2 {
            h.key(egui::Key::Escape);
            for _ in 0..40 {
                h.frame(Vec::new());
            }
            assert!(h.app.help.open && h.app.notice.is_some(), "a single Esc (guide open {}, message {})", h.app.help.open, h.app.notice.is_some());
        }
        esc_twice(&mut h);
        assert!(h.app.notice.is_some(), "two Esc close the guide and leave the message");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.notice.is_none(), "the next Esc closes the message");
    }

    /// With the guide over the message at the end of a copy nothing could be typed in its search
    /// field: the keys were all taken for the message (Piero, 2026-11-21). Now the text and the
    /// keys that edit it are the field's, and nothing else reads them: not the grid, not the
    /// message, which Enter does not close either.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn search_in_the_guide_over_a_message() {
        if own_folder("help::tests::search_in_the_guide_over_a_message") {
            return;
        }
        let (_, mut h) = guide_over("guide-search-message", copy_ended);
        assert!(h.app.help.typing, "the guide's field has the keyboard over the message");
        let chosen = (h.app.selected, h.app.marked.len(), h.app.dir.clone());
        // "ftp" is in the guide in every language: the CI runs in English.
        type_text(&mut h, "ftp");
        assert_eq!(h.app.help.query, "ftp", "typed in the field over the message");
        assert!(!h.app.help.hits.is_empty(), "and searched");
        h.key(egui::Key::ArrowLeft);
        type_text(&mut h, "x");
        assert_eq!(h.app.help.query, "ftxp", "the arrow moves in the field");
        h.key(egui::Key::Backspace);
        assert_eq!(h.app.help.query, "ftp");
        // Ctrl+A is the field's: all its text, replaced by the next letter; no file is marked.
        h.key_with(egui::Key::A, egui::Modifiers::COMMAND);
        type_text(&mut h, "z");
        assert_eq!(h.app.help.query, "z", "Ctrl+A selects the text of the field");
        for key in [egui::Key::ArrowRight, egui::Key::ArrowDown, egui::Key::End, egui::Key::Home, egui::Key::Delete, egui::Key::F5] {
            h.frame(vec![pressed(key, egui::Modifiers::NONE)]);
            assert!(!h.ctx.input(|i| i.events.iter().any(is_typed)), "{key:?} left in the input for what is under the guide");
            h.frame(Vec::new());
            assert_eq!((h.app.selected, h.app.marked.len(), h.app.dir.clone()), chosen, "{key:?} reached the grid");
            assert!(h.app.notice.is_some() && h.app.delete_confirm.is_none() && h.app.help.typing, "{key:?} reached the message, or left the field");
        }
        h.key(egui::Key::Enter);
        settle(&mut h);
        assert!(h.app.notice.is_some() && h.app.help.open && h.app.help.typing, "Enter in the guide's field closed the message, or left the field");
        // Tab goes round inside the guide (#311, W1.4): it leaves the field for the guide's next
        // control, and reaches neither the message nor the grid.
        h.frame(vec![pressed(egui::Key::Tab, egui::Modifiers::NONE)]);
        h.frame(Vec::new());
        assert_eq!((h.app.selected, h.app.marked.len(), h.app.dir.clone()), chosen, "Tab reached the grid");
        assert!(h.app.notice.is_some() && h.app.delete_confirm.is_none(), "Tab reached the message");
        let guide = egui::LayerId::new(egui::Order::Foreground, window_id());
        assert!(h.ctx.memory(|m| m.focused()).and_then(|id| h.ctx.read_response(id)).is_some_and(|r| r.layer_id == guide), "Tab left the guide");
        esc_twice(&mut h);
        assert!(h.app.notice.is_some(), "two Esc close the guide and leave the message");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.notice.is_none(), "the third Esc closes the message");
    }

    /// The same over a deletion asked: the search field takes what is typed, and Enter never
    /// deletes, neither in the field nor out of it.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn search_in_the_guide_over_a_deletion() {
        if own_folder("help::tests::search_in_the_guide_over_a_deletion") {
            return;
        }
        let (dir, mut h) = guide_over("guide-search-delete", |app| {
            let a = app.files().iter().find(|f| f.path.ends_with("a.jpg")).unwrap().path.clone();
            app.ask_delete_paths(vec![a], false, None);
        });
        assert!(h.app.help.typing, "the guide's field has the keyboard over the deletion");
        type_text(&mut h, "elimina");
        assert_eq!(h.app.help.query, "elimina", "typed in the field over the deletion");
        enter_answers_nothing(&mut h, &dir);
        assert!(h.app.help.typing, "Enter left the field");
        h.key(egui::Key::Delete);
        h.key(egui::Key::Backspace);
        assert_eq!(h.app.help.query, "elimin");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.help.open && !h.app.help.typing && h.app.delete_confirm.is_some(), "one Esc leaves the field only");
        // Out of the field the keys are the deletion's again, but Enter and Esc go to the guide.
        // After more than `DOUBLE_ESC`: the next Esc is a first one again.
        for _ in 0..40 {
            h.frame(Vec::new());
        }
        type_text(&mut h, "x");
        assert_eq!(h.app.help.query, "elimin", "typed out of the field");
        enter_answers_nothing(&mut h, &dir);
        esc_twice(&mut h);
        assert!(h.app.delete_confirm.is_some() && dir.join("a.jpg").exists(), "two Esc close the guide and leave the deletion asked");
        h.key(egui::Key::Escape);
        settle(&mut h);
        assert!(h.app.delete_confirm.is_none() && dir.join("a.jpg").exists(), "the third Esc cancels the deletion");
    }

    /// Over a page, as before: what is typed goes in the field, one Esc leaves it, and with the
    /// pointer on the guide the second closes the guide.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn two_esc_close_the_guide_over_a_page() {
        if own_folder("help::tests::two_esc_close_the_guide_over_a_page") {
            return;
        }
        let dir = folder("guide-page", &["a.jpg", "b.jpg"]);
        let mut h = Harness::new(&dir);
        let under = (h.app.dir.clone(), h.app.selected, h.app.viewing);
        h.key(egui::Key::F1);
        h.frame(Vec::new());
        assert!(h.app.help.open && h.app.help.typing && h.app.help.page == "grid", "the guide over the grid, its field with the keyboard");
        type_text(&mut h, "zoom");
        assert_eq!(h.app.help.query, "zoom");
        let on_guide = h.ctx.memory(|m| m.area_rect(window_id())).unwrap().center();
        h.frame(vec![egui::Event::PointerMoved(on_guide)]);
        esc_twice(&mut h);
        assert_eq!((h.app.dir.clone(), h.app.selected, h.app.viewing), under, "the grid under the guide");
    }

    /// The guide over a deletion asked: Enter is not the deletion's (it would delete the file
    /// under the guide), two Esc close the guide, and then the deletion answers again.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn enter_and_esc_with_the_guide_over_a_deletion() {
        if own_folder("help::tests::enter_and_esc_with_the_guide_over_a_deletion") {
            return;
        }
        let (dir, mut h) = guide_over("guide-delete", |app| {
            let a = app.files().iter().find(|f| f.path.ends_with("a.jpg")).unwrap().path.clone();
            app.ask_delete_paths(vec![a], false, None);
        });
        assert_eq!(h.app.help.page, "delete");
        enter_answers_nothing(&mut h, &dir);
        esc_twice(&mut h);
        assert!(h.app.delete_confirm.is_some(), "two Esc close the guide and leave the deletion asked");
        h.key(egui::Key::Escape);
        settle(&mut h);
        assert!(h.app.delete_confirm.is_none() && dir.join("a.jpg").exists(), "the next Esc cancels the deletion");
    }

    /// Enter with the guide over the deletion of a.jpg: a.jpg stays, asked, under the guide open.
    fn enter_answers_nothing(h: &mut Harness, dir: &std::path::Path) {
        h.key(egui::Key::Enter);
        settle(h);
        assert!(dir.join("a.jpg").exists(), "Enter with the guide on top deleted a.jpg under it");
        assert!(h.app.delete_confirm.is_some(), "Enter with the guide on top answered the deletion under it");
        assert!(h.app.help.open, "Enter closed the guide");
    }

    /// The guide open first, then a deletion asked: the guide is still on top, and takes Enter
    /// and Esc before the deletion.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_deletion_asked_under_the_guide_open() {
        if own_folder("help::tests::a_deletion_asked_under_the_guide_open") {
            return;
        }
        let dir = folder("guide-first", &["a.jpg", "b.jpg"]);
        let mut h = Harness::new(&dir);
        h.key(egui::Key::F1);
        h.frame(Vec::new());
        let a = h.app.files().iter().find(|f| f.path.ends_with("a.jpg")).unwrap().path.clone();
        h.app.ask_delete_paths(vec![a], false, None);
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        let layer = egui::LayerId::new(egui::Order::Foreground, window_id());
        assert!(h.app.help.open && h.ctx.memory(|m| m.is_above_modal_layer(layer)), "the guide over the deletion");
        enter_answers_nothing(&mut h, &dir);
        esc_twice(&mut h);
        assert!(h.app.delete_confirm.is_some(), "two Esc close the guide and leave the deletion asked");
    }

    /// Open location under the guide, its address filled in, the guide's search field with the
    /// keyboard (as F1 leaves it): Enter is the field's, the address is not opened.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn enter_in_the_guide_over_open_location() {
        if own_folder("help::tests::enter_in_the_guide_over_open_location") {
            return;
        }
        let other = folder("guide-address-other", &["c.jpg"]);
        let url = format!("{}/", other.display());
        let (_, mut h) = guide_over("guide-address", |app| app.open_address = Some(crate::address::AddressForm { url: url.clone(), ..crate::address::AddressForm::new() }));
        let dir = h.app.dir.clone();
        assert!(h.app.help.typing, "the guide's field has the keyboard");
        type_text(&mut h, "ftp");
        assert_eq!((h.app.help.query.as_str(), h.app.open_address.as_ref().map(|f| f.url.clone())), ("ftp", Some(url)), "typed in the guide's field, not in the address");
        h.key(egui::Key::Enter);
        settle(&mut h);
        assert!(h.app.open_address.is_some() && h.app.dir == dir, "Enter in the guide opened the address under it (folder {:?})", h.app.dir);
        esc_twice(&mut h);
        assert!(h.app.open_address.is_some(), "two Esc close the guide and leave Open location");
    }

    /// About is not a modal: the guide goes over it too, and takes Esc first.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn esc_closes_the_guide_over_about() {
        if own_folder("help::tests::esc_closes_the_guide_over_about") {
            return;
        }
        let (_, mut h) = guide_over("guide-about", |app| app.about = true);
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.help.open && h.app.about, "one Esc closes neither the guide nor About");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(!h.app.help.open && h.app.about, "two Esc close the guide and leave About");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(!h.app.about, "the next Esc closes About");
    }

    /// Options is no question and no modal: over it too the guide takes Esc first, two close it.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn esc_closes_the_guide_over_options() {
        if own_folder("help::tests::esc_closes_the_guide_over_options") {
            return;
        }
        let (_, mut h) = guide_over("guide-options", |app| app.options = true);
        type_text(&mut h, "tema");
        assert_eq!(h.app.help.query, "tema");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.help.open && h.app.options, "one Esc closes neither the guide nor Options");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(!h.app.help.open && h.app.options, "two Esc close the guide and leave Options");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(!h.app.options, "the next Esc closes Options");
    }

    /// The guide over the server manager, a window with fields, once the guide's field has let
    /// go of the keyboard: Esc is the guide's, not the manager's.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn esc_closes_the_guide_over_the_server_manager() {
        if own_folder("help::tests::esc_closes_the_guide_over_the_server_manager") {
            return;
        }
        let (_, mut h) = guide_over("guide-servers", |app| app.server_manager = Some(crate::address::AddressForm::manager()));
        h.ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(egui::Id::NULL)));
        h.frame(Vec::new());
        h.frame(Vec::new());
        esc_twice(&mut h);
        assert!(h.app.server_manager.is_some(), "two Esc close the guide and leave the server manager");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.server_manager.is_none(), "the next Esc closes the server manager");
    }

    /// #354, "Why do I have to wait?" and "Why through the computer?" as the first thing that opens
    /// the guide, the interface in Italian or in English: the section is found and the page goes
    /// down to it, as far as when the guide is open on that page already. It stayed at the top in
    /// Italian: the title was looked for in the English pages.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_section_opened_first_is_in_view() {
        if own_folder("help::tests::a_section_opened_first_is_in_view") {
            return;
        }
        for lang in ["it", "en"] {
            for id in ["phone-copy-why-section", "transfer-why-pc-section"] {
                let down = |first: bool| {
                    let mut h = Harness::new(&folder(&format!("guide-section-{lang}-{id}-{first}"), &["a.png"]));
                    h.app.i18n = crate::i18n::I18n::new(lang);
                    let title = h.app.i18n.t(id);
                    if !first {
                        h.app.open_help("phone");
                        settle(&mut h);
                    }
                    h.app.open_help_at("phone", &title);
                    for _ in 0..4 {
                        settle(&mut h);
                    }
                    assert_eq!((h.app.help.page, h.app.help.cache.active_match(&viewer_id())), ("phone", Some(0)), "{lang} {id}: the heading is not the match shown");
                    h.app.help.scrolled
                };
                let (first, again) = (down(true), down(false));
                assert!(again > 200.0, "{lang} {id}: the page did not go down to the section ({again})");
                assert!((first - again).abs() < 2.0, "{lang} {id}: opened first the page is at {first}, not at {again}");
            }
        }
    }

    /// Some frames for what a key started (a deletion in the background).
    fn settle(h: &mut Harness) {
        for _ in 0..10 {
            h.frame(Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    }
}
