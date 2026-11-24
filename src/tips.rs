//! Tips at startup (issue #113): at the first start of the day a window shows one tip about what
//! EyeCanSee does, with a picture of it, Back and Next to see the others, a link to the guide's
//! page and "Don't show at startup". On by default, also for who updates from an older version
//! (the key missing from `settings.json`); off from the box or from Options > General, until
//! turned on there again. The next start shows the tip after the last one seen.
//!
//! Keep them true: a change to a feature a tip names (a key, a menu, a window, a picture) changes
//! the tip in the same commit, `locales/*.ftl` and its picture (`tools/shots/tips/<key>.txt`, in
//! every language that has its own); a new main feature gets a tip at the end of `TIPS`, one
//! removed loses its tip.
//!
//! The pictures (issue #279): one for each tip, all of the same shape, the part of the window the
//! tip is about. Where the picture shows words the text names there is one for each language,
//! otherwise one without words for all; a language without its own gets the English ones.

use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::App;
use crate::help::{Picture, picture};
use crate::settings::Settings;

/// One tip: its Fluent ids are `tip-<key>-title` and `tip-<key>-text`, its picture
/// `docs/tips/<key>.webp` (`PICTURES`); `page` the guide's page that tells more.
pub(crate) struct Tip {
    pub(crate) key: &'static str,
    pub(crate) page: &'static str,
}

const fn tip(key: &'static str, page: &'static str) -> Tip {
    Tip { key, page }
}

/// The pictures of the tips, each `PICTURE` pixels (tools/shots/tips/, dark theme): first those
/// in English or without words, `docs/tips/<key>.webp`, then for each language its own,
/// `docs/tips/<language>/<key>.webp`.
macro_rules! pictures {
    ([$($name:literal),* $(,)?] $(, $lang:literal: [$($own:literal),* $(,)?])* $(,)?) => {
        const PICTURES: &[Picture] = &[
            $(("en", $name, include_bytes!(concat!("../docs/tips/", $name, ".webp"))),)*
            $($(($lang, $own, include_bytes!(concat!("../docs/tips/", $lang, "/", $own, ".webp"))),)*)*
        ];
    };
}

pictures![
    [
        "welcome", "help", "right-click", "phone", "servers", "viewer", "video", "editor", "cover", "resize", "convert-video",
        "bookmarks", "metadata", "models", "duplicates", "trash", "details", "slideshow", "addons", "pano",
    ],
    "it": [
        "welcome", "help", "right-click", "phone", "servers", "resize", "convert-video", "bookmarks", "metadata", "duplicates",
        "trash", "details", "addons", "pano",
    ],
];

/// The address egui knows the picture of a tip by, in a language (the bytes: `picture`).
fn picture_uri(lang: &str, key: &str) -> String {
    format!("bytes://tips/{lang}/{key}.webp")
}

/// The main features first, then the ones less in sight. New tips go at the end: who has seen
/// the others finds them next.
pub(crate) const TIPS: &[Tip] = &[
    tip("welcome", "start"),
    tip("help", "start"),
    tip("right-click", "start"),
    tip("phone", "phone"),
    tip("servers", "servers"),
    tip("viewer", "viewer"),
    tip("video", "video"),
    tip("editor", "editor"),
    tip("cover", "editor-cover"),
    tip("resize", "convert"),
    tip("convert-video", "convert-video"),
    tip("bookmarks", "bookmarks"),
    tip("metadata", "metadata"),
    tip("models", "model-3d"),
    tip("duplicates", "duplicates"),
    tip("trash", "delete"),
    tip("details", "options"),
    tip("slideshow", "slideshow"),
    tip("addons", "components"),
    tip("pano", "viewer"),
];

/// Today, as days since 1970 in UTC.
// ponytail: a UTC day, so near midnight the "day" can change an hour early or late; a local
// calendar would need a time-zone crate.
pub(crate) fn today() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400)
}

/// The tip to show at startup: tips on, none shown yet today, and EyeCanSee started on its own
/// (not to open a file or a folder another program passed, which the window would cover).
pub(crate) fn at_start(s: &Settings, today: u64, opened_with_address: bool) -> Option<usize> {
    (s.tips && !opened_with_address && s.tips_day != Some(today)).then(|| s.tip_next % TIPS.len())
}

/// Tip `i` was seen today: the next start, another day, shows the one after it.
pub(crate) fn seen(s: &mut Settings, i: usize, today: u64) {
    s.tips_day = Some(today);
    s.tip_next = (i + 1) % TIPS.len();
}

/// The tip shown, and the viewer of the guide's pages that draws its picture (it also gives egui
/// the loaders of the pictures, which until the guide is first opened are not there). One viewer
/// for every tip: a new one at each tip made the picture flash while it was loaded again.
pub(crate) struct Open {
    i: usize,
    cache: CommonMarkCache,
    /// The last Back (-1) or Next (1), and when (time of egui): the new tip slides in from that side.
    from: f32,
    since: f64,
    /// The language whose pictures egui was given.
    loaded: Option<&'static str>,
    /// What the texts measure, for the body's height.
    fit: Option<Fit>,
    /// → (`true`) or ← pressed, taken from the keys before the frame (`App::asking_keys`): with
    /// the tip open no key reaches the grid behind it (audit 1.13).
    turn: Option<bool>,
}

impl Open {
    fn new(i: usize) -> Self {
        Open { i, cache: CommonMarkCache::default(), from: 0.0, since: 0.0, loaded: None, fit: None, turn: None }
    }

    pub(crate) fn turn(&mut self, next: bool) {
        self.turn = Some(next);
    }
}

/// The height the text and the link of each tip take, in a language and at a width.
struct Fit {
    lang: &'static str,
    width: f32,
    below: Vec<f32>,
}

/// The window's content: as wide as a picture, less only in a window of the program too narrow
/// for it (`width_in`).
const WIDTH: f32 = 640.0;
const MIN_WIDTH: f32 = 320.0;
/// What the window and its frame take at the sides of the content.
const EDGES: f32 = 40.0;
/// The size of every picture, in pixels: one shape for all, so the texts start at the same height.
const PICTURE: egui::Vec2 = egui::vec2(640.0, 360.0);
/// Between the picture and the text.
const GAP: f32 = 6.0;
/// The slide from the side: how far and how long.
const SLIDE: f32 = 60.0;
const SLIDE_SECS: f64 = 0.2;

/// The width of the content in a window of the program that wide.
fn width_in(window: f32) -> f32 {
    WIDTH.min(window - EDGES).max(MIN_WIDTH)
}

/// The height of a picture shown that wide.
fn picture_height(width: f32) -> f32 {
    (width * PICTURE.y / PICTURE.x).round()
}

/// The height of the body: the picture and the tallest text with its link (`below`, one for each
/// tip). The same for every tip, so Back and Next stay where they are; under the shorter texts
/// only their difference from the tallest stays empty.
fn body_height(width: f32, below: &[f32]) -> f32 {
    picture_height(width) + GAP + below.iter().copied().fold(0.0, f32::max).ceil()
}

/// What a click in the window asks.
enum Choice {
    Back,
    Next,
    Close,
    Guide,
}

impl App {
    /// At startup, after the first folder is opened: the tip of the day, if due.
    pub(crate) fn tip_at_start(&mut self, opened_with_address: bool) {
        // The pictures of the guide always start on a folder given (tools/shots.sh).
        #[cfg(feature = "shots")]
        let opened_with_address = opened_with_address && !crate::shots::active();
        let day = today();
        let i = at_start(&self.settings, day, opened_with_address);
        self.tip = i.map(Open::new);
        if let Some(i) = i {
            log!("tips: showing {}", TIPS[i].key);
            seen(&mut self.settings, i, day);
            self.settings.save();
        }
    }

    /// Tip `i` opened now, already marked as seen: after the first-start window (`first_start`).
    pub(crate) fn tip_show(&mut self, i: usize) {
        log!("tips: showing {}", TIPS[i].key);
        self.tip = Some(Open::new(i));
    }

    /// The tip's window, while open.
    pub(crate) fn tip_window(&mut self, ctx: &egui::Context) {
        let Some(open) = &mut self.tip else { return };
        let (i, lang) = (open.i, self.i18n.code);
        if open.loaded != Some(lang) {
            for tip in TIPS {
                if let Some(bytes) = picture(PICTURES, lang, tip.key) {
                    ctx.include_bytes(picture_uri(lang, tip.key), bytes);
                }
            }
            open.loaded = Some(lang);
        }
        let tip = &TIPS[i];
        let t = |id: &str| self.i18n.t(id);
        let text_of = |tip: &Tip| t(&format!("tip-{}-text", tip.key));
        let (title, text, link) = (t(&format!("tip-{}-title", tip.key)), text_of(tip), t("tip-guide"));
        let width = width_in(ctx.content_rect().width());
        // Every text, only when they are to be measured: another language, another width.
        let fits = open.fit.as_ref().is_some_and(|f| f.lang == lang && f.width == width);
        let texts: Vec<String> = if fits { Vec::new() } else { TIPS.iter().map(text_of).collect() };
        let count = self.i18n.t_args("tip-count", &[("n", &(i + 1).to_string()), ("of", &TIPS.len().to_string())]);
        let (back, next, close, off_text, off_hint) = (t("tip-back"), t("tip-next"), t("notice-close"), t("tip-off"), t("tip-off-hint"));
        let mut off = !self.settings.tips;
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("tip")).show(ctx, |ui| {
            ui.set_width(width);
            crate::save_dialogs::Head::new(&title).show(ui);
            ui.add_space(4.0);
            if !fits {
                open.fit = Some(Fit { lang, width, below: measure(ui, width, &texts, &link) });
            }
            if tip_body(ui, open, &picture_uri(lang, tip.key), &text, &link) {
                choice = Some(Choice::Guide);
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.weak(&count);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Close highlighted when the tip opens (#311); Tab goes along them as on screen.
                    let buttons = [(&close, Choice::Close), (&next, Choice::Next), (&back, Choice::Back)];
                    let rows: Vec<egui::Response> = buttons.iter().map(|(label, _)| ui.button(label.as_str())).collect();
                    if let Some(i) = crate::confirm::keys(ui, "tip", &rows, 0) {
                        choice = buttons.into_iter().nth(i).map(|(_, c)| c);
                    }
                });
            });
            ui.add_space(4.0);
            ui.checkbox(&mut off, off_text).on_hover_text(off_hint);
        });
        // Esc closes it, ← and → turn it, only with the tip on top (D6); Enter and Space press the
        // highlighted button.
        let turn = open.turn.take().filter(|_| crate::confirm::on_top(ctx, "tip"));
        let close = (crate::confirm::answer(ctx, &mut self.key_answer, "tip") == Some(false)).then_some(Choice::Close);
        choice = choice.or(close).or(turn.map(|next| if next { Choice::Next } else { Choice::Back }));
        if off == self.settings.tips {
            self.settings.tips = !off;
            log!("tips: at startup {}", self.settings.tips);
            self.settings.save();
        }
        let (len, now) = (TIPS.len(), ctx.input(|k| k.time));
        match choice {
            Some(Choice::Next) => self.tip_shown((i + 1) % len, 1.0, now),
            Some(Choice::Back) => self.tip_shown((i + len - 1) % len, -1.0, now),
            Some(Choice::Close) => self.tip = None,
            Some(Choice::Guide) => {
                self.tip = None;
                self.open_help(tip.page);
            }
            None => {}
        }
    }

    /// Back or Next: the tip shown is the last one seen.
    fn tip_shown(&mut self, i: usize, from: f32, now: f64) {
        if let Some(open) = &mut self.tip {
            (open.i, open.from, open.since) = (i, from, now);
        }
        seen(&mut self.settings, i, today());
        self.settings.save();
    }
}

/// The height the text and the link of each tip take at this width: laid out as `tip_body` does,
/// not shown.
fn measure(ui: &mut egui::Ui, width: f32, texts: &[String], link: &str) -> Vec<f32> {
    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, 0.0));
    let unseen = || egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)).invisible();
    texts
        .iter()
        .map(|text| {
            let mut probe = ui.new_child(unseen());
            below(&mut probe, text, link);
            probe.min_rect().height()
        })
        .collect()
}

/// Under the picture: the text and the link to the guide. `true`: the link was clicked.
fn below(ui: &mut egui::Ui, text: &str, link: &str) -> bool {
    crate::save_dialogs::notice_text(ui, text);
    ui.add_space(4.0);
    ui.link(link).clicked()
}

/// The picture, as large as `ui`, with a thin border: in the dark theme a dark picture would not
/// show where it ends. Drawn by the viewer of the guide's pages, which also gives egui the loaders
/// of the pictures.
fn picture_box(ui: &mut egui::Ui, cache: &mut CommonMarkCache, uri: &str) {
    let rect = ui.max_rect();
    CommonMarkViewer::new().max_image_width(Some(rect.width() as usize)).show(ui, cache, &format!("![]({uri})"));
    ui.painter().rect_stroke(rect, 0.0, ui.visuals().window_stroke, egui::StrokeKind::Inside);
}

/// Picture, text and link of the tip, in a box of fixed size (`body_height`); after Back or Next
/// it slides in from that side while it fades in. `true`: the link was clicked.
fn tip_body(ui: &mut egui::Ui, open: &mut Open, uri: &str, text: &str, link: &str) -> bool {
    let width = ui.available_width();
    let height = body_height(width, open.fit.as_ref().map_or(&[], |f| &f.below));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let done = ((ui.input(|k| k.time) - open.since) / SLIDE_SECS).clamp(0.0, 1.0) as f32;
    if done < 1.0 {
        ui.ctx().request_repaint();
    }
    // Fast at first, slow at the end (ease-out cubic).
    let eased = 1.0 - (1.0 - done).powi(3);
    let slides = open.from != 0.0;
    let moved = rect.translate(egui::vec2(if slides { open.from * SLIDE * (1.0 - eased) } else { 0.0 }, 0.0));
    // The picture and, under it, the text: each in its own place, whatever the other takes.
    let (picture, text_place) = moved.split_top_bottom_at_y(moved.top() + picture_height(width));
    let mut part = |place: egui::Rect| {
        let mut part = ui.new_child(egui::UiBuilder::new().max_rect(place).layout(egui::Layout::top_down(egui::Align::Min)));
        part.set_clip_rect(rect.intersect(ui.clip_rect()));
        if slides {
            part.multiply_opacity(eased);
        }
        part
    };
    picture_box(&mut part(picture), &mut open.cache, uri);
    below(&mut part(text_place.with_min_y(text_place.top() + GAP)), text, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The most that stays empty under the shortest text, in points ("about 54" in issue #279).
    const EMPTY_AT_MOST: f32 = 57.0; // measured: texts from 52 to 109 points, English and Italian

    /// On at the very first start, and for who updates from a settings.json without the key.
    #[test]
    fn shown_at_first_start_and_after_an_update() {
        assert_eq!(at_start(&Settings::default(), 20_000, false), Some(0));
        let old: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50}"#).unwrap();
        assert_eq!(at_start(&old, 20_000, false), Some(0));
    }

    /// Once a day, the next tip each day, round again after the last.
    #[test]
    fn once_a_day_and_in_turn() {
        let mut s = Settings::default();
        seen(&mut s, 0, 20_000);
        assert_eq!(at_start(&s, 20_000, false), None, "not twice the same day");
        assert_eq!(at_start(&s, 20_001, false), Some(1));
        seen(&mut s, TIPS.len() - 1, 20_001);
        assert_eq!(at_start(&s, 20_002, false), Some(0));
    }

    /// Off (from the box or from Options) stays off, saved in settings.json, until turned on.
    #[test]
    fn off_stays_off() {
        let s = Settings { tips: false, ..Settings::default() };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(at_start(&back, 20_000, false), None);
        assert_eq!(at_start(&Settings { tips: true, ..back }, 20_000, false), Some(0));
    }

    /// Not over a file or folder opened from another program.
    #[test]
    fn not_when_opened_with_an_address() {
        assert_eq!(at_start(&Settings::default(), 20_000, true), None);
    }

    /// Every tip has its texts (the other languages have the same ids: `i18n` checks it), a page
    /// that exists and its picture, of the one size, in every language: its own or the English.
    #[test]
    fn every_tip_complete() {
        let en = crate::i18n::I18n::new("en");
        for tip in TIPS {
            for part in ["title", "text"] {
                let id = format!("tip-{}-{part}", tip.key);
                assert_ne!(en.t(&id), id, "{id} missing");
            }
            assert!(crate::help::PAGES.iter().any(|p| p.1 == tip.page), "{}: page", tip.key);
            for (lang, _) in crate::i18n::languages() {
                assert!(picture(PICTURES, lang, tip.key).is_some(), "{}: no picture for {lang}", tip.key);
            }
        }
        for (lang, name, bytes) in PICTURES {
            assert!(TIPS.iter().any(|t| t.key == *name), "{lang}/{name}: a picture of no tip");
            let size = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().unwrap().into_dimensions().unwrap();
            assert_eq!(size, (PICTURE.x as u32, PICTURE.y as u32), "{lang}/{name}: every picture has the same size");
        }
    }

    /// The names of the pictures of a language.
    fn names(lang: &str) -> Vec<&'static str> {
        let mut names: Vec<_> = PICTURES.iter().filter(|p| p.0 == lang).map(|p| p.1).collect();
        names.sort_unstable();
        names
    }

    /// The pictures in `docs/tips` are those built in, and a language with its own pictures has
    /// all those with words: the same ones as every other such language.
    #[test]
    fn pictures_on_disk_and_by_language() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/tips");
        let listed = |dir: &std::path::Path| {
            let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort_unstable();
            names
        };
        let own: Vec<&str> = crate::i18n::languages().into_iter().map(|l| l.0).filter(|l| *l != "en" && !names(l).is_empty()).collect();
        let files = |lang: &str| names(lang).iter().map(|n| format!("{n}.webp")).collect::<Vec<_>>();
        let mut top = files("en");
        top.extend(own.iter().map(|l| l.to_string()));
        top.sort_unstable();
        assert_eq!(listed(&dir), top, "docs/tips: the English pictures and a folder for each language with its own");
        for lang in &own {
            assert_eq!(listed(&dir.join(lang)), files(lang), "docs/tips/{lang}");
            assert_eq!(names(lang), names(own[0]), "{lang}: the pictures with words are the same in every language");
            assert!(names(lang).iter().all(|n| names("en").contains(n)), "{lang}: a picture without its English one");
        }
        assert!(PICTURES.iter().all(|p| p.0 == "en" || own.contains(&p.0)), "pictures of a language EyeCanSee does not speak");
    }

    /// The content is as wide as a picture, and narrower only in a narrow window; the body as tall
    /// as the picture and the tallest text.
    #[test]
    fn size_of_the_content() {
        assert_eq!(width_in(1200.0), 640.0);
        assert_eq!(width_in(600.0), 560.0);
        assert_eq!(width_in(200.0), MIN_WIDTH);
        assert_eq!(picture_height(640.0), 360.0);
        assert_eq!(picture_height(560.0), 315.0);
        assert_eq!(body_height(640.0, &[70.0, 104.2, 88.0]), 360.0 + GAP + 105.0);
    }

    /// Back and Next stay where they are: the window has the same place and size for every tip,
    /// in every language, and under the shortest text little stays empty (issue #279: 124 points
    /// on average, 392 at most, when the body was 500 points for pictures of any height).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn window_the_same_for_every_tip() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::window_the_same_for_every_tip") {
            return;
        }
        let mut h = Harness::new(&folder("tip-window", &["a.jpg"]));
        for (lang, _) in crate::i18n::languages() {
            h.app.i18n = crate::i18n::I18n::new(lang);
            h.app.tip = Some(Open::new(0));
            let mut places = Vec::new();
            for tip in TIPS {
                // The slide from the side over, the window settled.
                for _ in 0..4 {
                    h.frame(Vec::new());
                }
                let place = h.ctx.memory(|m| m.area_rect(egui::Id::unique("tip"))).expect("the tip's window");
                places.push((tip.key, place));
                h.key(egui::Key::ArrowRight);
            }
            for (key, place) in &places {
                assert_eq!(*place, places[0].1, "{lang}: the window of {key} is not where that of {} is", places[0].0);
            }
            let fit = h.app.tip.as_ref().and_then(|t| t.fit.as_ref()).expect("the texts measured");
            assert_eq!((fit.lang, fit.width, fit.below.len()), (lang, WIDTH, TIPS.len()));
            let (short, tall) = fit.below.iter().fold((f32::MAX, 0.0_f32), |(a, b), h| (a.min(*h), b.max(*h)));
            println!("{lang}: text and link from {short} to {tall} points, body {}", body_height(WIDTH, &fit.below));
            assert!(tall - short <= EMPTY_AT_MOST, "{lang}: {} points empty under the shortest text", tall - short);
        }
    }

    /// The guide opened by F1 over a tip, its search field with the keyboard: the arrows move in
    /// the field and do not turn the tips under it (the tip reads them whoever has the keyboard).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn arrows_in_the_guide_do_not_turn_the_tip_under_it() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::arrows_in_the_guide_do_not_turn_the_tip_under_it") {
            return;
        }
        let mut h = Harness::new(&folder("tip-guide", &["a.jpg"]));
        h.app.tip = Some(Open::new(0));
        h.frame(Vec::new());
        h.key(egui::Key::ArrowRight);
        assert_eq!(h.app.tip.as_ref().map(|t| t.i), Some(1), "without the guide the arrow turns the tip");
        h.key(egui::Key::F1);
        h.frame(Vec::new());
        assert!(h.app.help.in_field(), "the guide open over the tip, its field with the keyboard");
        for key in [egui::Key::ArrowRight, egui::Key::ArrowLeft, egui::Key::ArrowRight] {
            h.key(key);
            h.frame(Vec::new());
            assert_eq!(h.app.tip.as_ref().map(|t| t.i), Some(1), "{key:?} in the guide's field turned the tip");
        }
    }

    /// Audit 1.13: with the tip open, the keys also reached the grid behind it: → turned the tip
    /// and moved the selection, Enter then opened the picture behind the window, Backspace went
    /// up, Del asked to delete, F2 renamed. Now (D6) ← and → turn the tip, Enter and Esc close it,
    /// the rest reaches nothing.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn keys_go_to_the_tip_only() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::keys_go_to_the_tip_only") {
            return;
        }
        let mut h = Harness::new(&folder("tip-keys/inner", &["a.png", "b.png", "c.png"]));
        h.app.tip_show(0);
        h.frame(Vec::new());
        let grid = |a: &crate::App| (a.selected, a.cursor, a.up_selected, a.marked.len(), a.dir.clone(), a.viewing, a.delete_confirm.is_some(), a.rename.is_some());
        let before = grid(&h.app);
        let tip = |h: &Harness| h.app.tip.as_ref().map(|t| t.i);
        h.key(egui::Key::ArrowRight);
        assert_eq!(tip(&h), Some(1), "→ shows the next tip");
        assert_eq!(grid(&h.app), before, "→ reached the grid");
        h.key(egui::Key::ArrowLeft);
        assert_eq!(tip(&h), Some(0), "← shows the tip before");
        let keys = [
            (egui::Key::ArrowDown, egui::Modifiers::NONE),
            (egui::Key::End, egui::Modifiers::NONE),
            (egui::Key::Backspace, egui::Modifiers::NONE),
            (egui::Key::ArrowUp, egui::Modifiers::ALT),
            (egui::Key::A, egui::Modifiers::COMMAND),
            (egui::Key::Delete, egui::Modifiers::NONE),
            (egui::Key::F2, egui::Modifiers::NONE),
            (egui::Key::F5, egui::Modifiers::NONE),
        ];
        for (key, modifiers) in keys {
            h.key_with(key, modifiers);
            h.frame(Vec::new());
            assert_eq!(grid(&h.app), before, "{key:?} reached the grid");
            assert_eq!(tip(&h), Some(0), "{key:?} changed the tip");
        }
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        assert_eq!(tip(&h), None, "Enter closes the tip");
        assert_eq!(grid(&h.app), before, "Enter reached the grid");
        h.app.tip_show(2);
        h.frame(Vec::new());
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert_eq!(tip(&h), None, "Esc closes the tip");
        assert_eq!(grid(&h.app), before, "Esc reached the grid");
    }

    /// D6, condition: Enter that presses Done in the first-start window does not also close the
    /// tip that comes right after it (`first_start_closed`, drawn after `tip_window`: the tip is
    /// born at the end of the frame of the key), neither when the key is let go nor when it is held
    /// down (the system repeats it).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn enter_on_done_does_not_close_the_tip_after_it() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::enter_on_done_does_not_close_the_tip_after_it") {
            return;
        }
        let mut h = Harness::new(&folder("tip-done", &["a.png"]));
        let enter = |pressed, repeat| egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed, repeat, modifiers: egui::Modifiers::NONE };
        // The frame of Enter on Done: no tip yet, the key is the window's; Done opens the tip.
        h.frame(vec![enter(true, false)]);
        h.app.tip_show(0);
        h.frame(vec![enter(true, true)]);
        h.frame(vec![enter(true, true), enter(false, false)]);
        h.frame(Vec::new());
        assert!(h.app.tip.is_some(), "the Enter of Done closed the tip");
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        assert!(h.app.tip.is_none(), "a new Enter closes it");
    }

    /// Audit 1.13, what was left (the folder that went up by itself behind the tip): egui keeps
    /// what is under a modal from the mouse only from the frame after the modal is drawn, so a
    /// click in the tip's first frame reached the grid, the tree or the toolbar behind it.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn no_click_behind_the_tip_in_its_first_frame() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::no_click_behind_the_tip_in_its_first_frame") {
            return;
        }
        let mut h = Harness::new(&folder("tip-click", &["a.png"]));
        let click = |pressed| egui::Event::PointerButton { pos: egui::pos2(20.0, 60.0), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        let after_hook = |h: &mut Harness| {
            let mut raw = egui::RawInput { events: vec![click(true), click(false)], ..Default::default() };
            eframe::App::raw_input_hook(&mut h.app, &h.ctx, &mut raw);
            raw.events.len()
        };
        h.app.tip_show(0);
        assert_eq!(after_hook(&mut h), 0, "a click in the tip's first frame reaches what is behind it");
        h.frame(Vec::new());
        h.frame(Vec::new());
        assert_eq!(after_hook(&mut h), 2, "once the tip is on top, egui keeps the click from what is behind");
    }

    /// #311: Back, Next and Close are one row of buttons, Close highlighted when the tip opens.
    /// Tab and Shift+Tab go along them as they are on screen, Enter and Space press the one
    /// highlighted; ↑ and ↓ do nothing here (← and → turn the tip, as before).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn enter_presses_the_highlighted_button() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::enter_presses_the_highlighted_button") {
            return;
        }
        let mut h = Harness::new(&folder("tip-buttons", &["a.png"]));
        let tip = |h: &Harness| h.app.tip.as_ref().map(|t| t.i);
        let open = |h: &mut Harness, i| {
            h.frame(Vec::new());
            h.app.tip_show(i);
            h.frame(Vec::new());
            h.frame(Vec::new());
        };
        // Close, then Tab round to Back: Enter shows the tip before.
        open(&mut h, 2);
        h.key(egui::Key::Tab);
        h.key(egui::Key::Enter);
        assert_eq!(tip(&h), Some(1), "Tab from Close goes to Back, and Enter presses it");
        // Shift+Tab from Close to Next: Space shows the tip after.
        open(&mut h, 2);
        h.key_with(egui::Key::Tab, egui::Modifiers::SHIFT);
        h.key(egui::Key::Space);
        assert_eq!(tip(&h), Some(3), "Shift+Tab from Close goes to Next, and Space presses it");
        // ↑ and ↓ leave Close highlighted: Enter closes.
        open(&mut h, 2);
        h.key(egui::Key::ArrowDown);
        h.key(egui::Key::ArrowUp);
        h.key(egui::Key::ArrowDown);
        assert_eq!(tip(&h), Some(2), "↑ and ↓ turned the tip");
        h.key(egui::Key::Enter);
        assert_eq!(tip(&h), None, "Close, highlighted, closes the tip");
    }

    /// #311: Tab reaches the check box under the buttons and the link above them, in the order
    /// they are on screen: Tab from Close is the check box, which Space ticks (once, the tip stays);
    /// Tab from there goes round to the link, which Enter follows to the guide.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn tab_reaches_the_check_box_and_the_link() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::tab_reaches_the_check_box_and_the_link") {
            return;
        }
        let mut h = Harness::new(&folder("tip-tab", &["a.png"]));
        h.frame(Vec::new());
        h.app.settings.tips = true;
        h.app.tip_show(2);
        h.frame(Vec::new());
        h.frame(Vec::new());
        h.key(egui::Key::Tab);
        h.key(egui::Key::Space);
        assert!(!h.app.settings.tips, "Tab from Close goes to Don't show at startup, and Space ticks it");
        assert_eq!(h.app.tip.as_ref().map(|t| t.i), Some(2), "the tip stays");
        h.key(egui::Key::Space);
        assert!(h.app.settings.tips, "Space again takes the tick away");
        h.key(egui::Key::Tab);
        h.key(egui::Key::Enter);
        assert!(h.app.tip.is_none() && h.app.help.open, "Tab from the check box goes round to the link, and Enter opens the guide");
    }

    /// #311: ↑ and ↓ do nothing in the tip, but a message over it keeps them for its buttons: ↑
    /// from Close (highlighted) to Save as…, which Enter presses.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn arrows_of_a_message_over_the_tip() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("tips::tests::arrows_of_a_message_over_the_tip") {
            return;
        }
        let mut h = Harness::new(&folder("tip-message", &["a.png"]));
        h.app.tip_show(0);
        h.frame(Vec::new());
        h.app.notice = Some(crate::save_dialogs::Notice { save_as: Some(crate::actions::Act::Save), ..crate::save_dialogs::Notice::new("Title".into(), "Text".into()) });
        h.frame(Vec::new());
        h.frame(Vec::new());
        h.key(egui::Key::ArrowUp);
        h.key(egui::Key::Enter);
        assert!(h.app.notice.is_none(), "Enter closes the message");
        assert!(h.app.tip.is_some(), "the tip under it stays");
        assert!(h.app.act.is_some(), "↑ over the tip did not reach the buttons of the message");
    }
}
