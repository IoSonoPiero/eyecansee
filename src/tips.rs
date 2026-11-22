//! Tips at startup (issue #113): at the first start of the day a window shows one tip about what
//! EyeCanSee does, with a picture of the guide or the steps, Back and Next to see the others, a
//! link to the guide's page and "Don't show at startup". On by default, also for who updates from
//! an older version (the key missing from `settings.json`); off from the box or from Options >
//! General, until turned on there again. The next start shows the tip after the last one seen.
//!
//! Keep them true: a change to a feature a tip names (a key, a menu, a window, a picture) changes
//! the tip in the same commit, `locales/*.ftl` and its picture (`tools/shots/tips/`, or the guide's);
//! a new main feature gets a tip at the end of `TIPS`, one removed loses its tip.

use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::App;
use crate::help::{Picture, picture};
use crate::settings::Settings;

/// One tip: its Fluent ids are `tip-<key>-title` and `tip-<key>-text`; `image` a picture of the
/// tips (`PICTURES`) or of the guide (`help::IMAGES`), `page` the guide's page that tells more.
pub(crate) struct Tip {
    pub(crate) key: &'static str,
    pub(crate) image: Option<&'static str>,
    pub(crate) page: &'static str,
}

const fn tip(key: &'static str, image: Option<&'static str>, page: &'static str) -> Tip {
    Tip { key, image, page }
}

/// Pictures made for the tips only (tools/shots/tips/, dark theme): first those in English or
/// without words, `docs/tips/<name>.webp`, then for each language its own,
/// `docs/tips/<language>/<name>.webp`. A language without its own gets the English ones.
macro_rules! pictures {
    ([$($name:literal),* $(,)?] $(, $lang:literal: [$($own:literal),* $(,)?])* $(,)?) => {
        const PICTURES: &[Picture] = &[
            $(("en", $name, include_bytes!(concat!("../docs/tips/", $name, ".webp"))),)*
            $($(($lang, $own, include_bytes!(concat!("../docs/tips/", $lang, "/", $own, ".webp"))),)*)*
        ];
    };
}

pictures![["help", "right-click", "cover", "details", "metadata", "slideshow", "addons"]];

/// The address egui knows a picture by, in a language: the tips' own, otherwise the guide's.
fn picture_uri(lang: &str, name: &str) -> String {
    if PICTURES.iter().any(|p| p.1 == name) { format!("bytes://tips/{lang}/{name}.webp") } else { crate::help::image_uri(name) }
}

/// The main features first, then the ones less in sight. New tips go at the end: who has seen
/// the others finds them next.
pub(crate) const TIPS: &[Tip] = &[
    tip("welcome", Some("start"), "start"),
    tip("help", Some("help"), "start"),
    tip("right-click", Some("right-click"), "start"),
    tip("phone", Some("phone"), "phone"),
    tip("servers", Some("server-manager"), "servers"),
    tip("viewer", Some("viewer"), "viewer"),
    tip("video", Some("video"), "video"),
    tip("editor", Some("editor"), "editor"),
    tip("cover", Some("cover"), "editor-cover"),
    tip("resize", Some("convert"), "convert"),
    tip("convert-video", Some("convert-video"), "convert-video"),
    tip("bookmarks", Some("bookmarks-menu"), "bookmarks"),
    tip("metadata", Some("metadata"), "metadata"),
    tip("models", Some("model-3d"), "model-3d"),
    tip("duplicates", Some("duplicates"), "duplicates"),
    tip("trash", Some("trash-strip"), "delete"),
    tip("details", Some("details"), "options"),
    tip("slideshow", Some("slideshow"), "slideshow"),
    tip("addons", Some("addons"), "components"),
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
}

impl Open {
    fn new(i: usize) -> Self {
        Open { i, cache: CommonMarkCache::default(), from: 0.0, since: 0.0, loaded: None }
    }
}

/// The window's content: as wide as the guide's pictures, as tall as a picture and the longest
/// text, the same for every tip so the buttons stay where they are.
const WIDTH: f32 = 480.0;
const BODY: f32 = 500.0;
/// The slide from the side: how far and how long.
const SLIDE: f32 = 60.0;
const SLIDE_SECS: f64 = 0.2;

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
        crate::help::load_images(ctx);
        if open.loaded != Some(lang) {
            for name in PICTURES.iter().filter(|p| p.0 == "en").map(|p| p.1) {
                if let Some(bytes) = picture(PICTURES, lang, name) {
                    ctx.include_bytes(picture_uri(lang, name), bytes);
                }
            }
            open.loaded = Some(lang);
        }
        let tip = &TIPS[i];
        let t = |id: &str| self.i18n.t(id);
        let (title, text, link) = (t(&format!("tip-{}-title", tip.key)), t(&format!("tip-{}-text", tip.key)), t("tip-guide"));
        let count = self.i18n.t_args("tip-count", &[("n", &(i + 1).to_string()), ("of", &TIPS.len().to_string())]);
        let (back, next, close, off_text, off_hint) = (t("tip-back"), t("tip-next"), t("notice-close"), t("tip-off"), t("tip-off-hint"));
        let mut off = !self.settings.tips;
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("tip")).show(ctx, |ui| {
            ui.set_width(WIDTH);
            crate::save_dialogs::Head::new(&title).show(ui);
            ui.add_space(4.0);
            if tip_body(ui, open, lang, &text, &link) {
                choice = Some(Choice::Guide);
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.weak(&count);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (label, c) in [(&close, Choice::Close), (&next, Choice::Next), (&back, Choice::Back)] {
                        if ui.button(label).clicked() {
                            choice = Some(c);
                        }
                    }
                });
            });
            ui.add_space(4.0);
            ui.checkbox(&mut off, off_text).on_hover_text(off_hint);
        });
        let keys = ctx.input(|k| (k.key_pressed(egui::Key::Escape), k.key_pressed(egui::Key::ArrowRight), k.key_pressed(egui::Key::ArrowLeft)));
        choice = choice.or(match keys {
            (true, _, _) => Some(Choice::Close),
            (_, true, _) => Some(Choice::Next),
            (_, _, true) => Some(Choice::Back),
            _ => None,
        });
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

/// Picture, text and link of the tip, in a box of fixed size; after Back or Next it slides in from
/// that side while it fades in. `true`: the link was clicked.
fn tip_body(ui: &mut egui::Ui, open: &mut Open, lang: &str, text: &str, link: &str) -> bool {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(WIDTH, BODY), egui::Sense::hover());
    let done = ((ui.input(|k| k.time) - open.since) / SLIDE_SECS).clamp(0.0, 1.0) as f32;
    if done < 1.0 {
        ui.ctx().request_repaint();
    }
    // Fast at first, slow at the end (ease-out cubic).
    let eased = 1.0 - (1.0 - done).powi(3);
    let shift = if open.from == 0.0 { 0.0 } else { open.from * SLIDE * (1.0 - eased) };
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(rect.translate(egui::vec2(shift, 0.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    body.set_clip_rect(rect.intersect(ui.clip_rect()));
    if open.from != 0.0 {
        body.multiply_opacity(eased);
    }
    if let Some(name) = TIPS[open.i].image {
        body.vertical_centered(|ui| {
            CommonMarkViewer::new().max_image_width(Some(WIDTH as usize)).show(ui, &mut open.cache, &format!("![]({})", picture_uri(lang, name)));
        });
    }
    body.add_space(6.0);
    crate::save_dialogs::notice_text(&mut body, text);
    body.add_space(4.0);
    body.link(link).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Every tip has its texts (the other languages have the same ids: `i18n` checks it), a
    /// picture of the guide and a page that exist.
    #[test]
    fn every_tip_complete() {
        let en = crate::i18n::I18n::new("en");
        for tip in TIPS {
            for part in ["title", "text"] {
                let id = format!("tip-{}-{part}", tip.key);
                assert_ne!(en.t(&id), id, "{id} missing");
            }
            // In every language: its own picture or the English one, or a picture of the guide.
            for (lang, _) in crate::i18n::languages() {
                let known = |n: &str| picture(PICTURES, lang, n).is_some() || crate::help::IMAGES.iter().any(|(name, _)| *name == n);
                assert!(tip.image.is_none_or(known), "{}: picture for {lang}", tip.key);
            }
            assert!(crate::help::PAGES.iter().any(|p| p.1 == tip.page), "{}: page", tip.key);
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
}
