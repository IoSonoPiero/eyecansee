//! Tips at startup (issue #113): at the first start of the day a window shows one tip about what
//! EyeCanSee does, with a picture of the guide or the steps, Back and Next to see the others, a
//! link to the guide's page and "Don't show at startup". On by default, also for who updates from
//! an older version (the key missing from `settings.json`); off from the box or from Options >
//! General, until turned on there again. The next start shows the tip after the last one seen.

use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::App;
use crate::settings::Settings;

/// One tip: its Fluent ids are `tip-<key>-title` and `tip-<key>-text`; `image` a picture of the
/// guide (`help::IMAGES`), `page` the guide's page that tells more.
pub(crate) struct Tip {
    pub(crate) key: &'static str,
    pub(crate) image: Option<&'static str>,
    pub(crate) page: &'static str,
}

const fn tip(key: &'static str, image: Option<&'static str>, page: &'static str) -> Tip {
    Tip { key, image, page }
}

/// The main features first, then the ones less in sight. New tips go at the end: who has seen
/// the others finds them next.
pub(crate) const TIPS: &[Tip] = &[
    tip("welcome", Some("start"), "start"),
    tip("help", None, "start"),
    tip("right-click", None, "start"),
    tip("phone", Some("phone"), "phone"),
    tip("servers", Some("server-manager"), "servers"),
    tip("viewer", Some("viewer"), "viewer"),
    tip("video", Some("video"), "video"),
    tip("editor", Some("editor"), "editor"),
    tip("cover", None, "editor-cover"),
    tip("resize", Some("convert"), "convert"),
    tip("convert-video", Some("convert-video"), "convert-video"),
    tip("bookmarks", Some("bookmarks-menu"), "bookmarks"),
    tip("metadata", Some("metadata-panel"), "metadata"),
    tip("models", Some("model-3d"), "model-3d"),
    tip("duplicates", Some("duplicates"), "duplicates"),
    tip("trash", Some("trash-strip"), "delete"),
    tip("details", None, "options"),
    tip("slideshow", None, "slideshow"),
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
/// the loaders of the pictures, which until the guide is first opened are not there).
pub(crate) struct Open {
    i: usize,
    cache: CommonMarkCache,
}

impl Open {
    fn new(i: usize) -> Self {
        Open { i, cache: CommonMarkCache::default() }
    }
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

    /// The tip's window, while open.
    pub(crate) fn tip_window(&mut self, ctx: &egui::Context) {
        let Some(open) = &mut self.tip else { return };
        let i = open.i;
        crate::help::load_images(ctx);
        let tip = &TIPS[i];
        let t = |id: &str| self.i18n.t(id);
        let (title, text) = (t(&format!("tip-{}-title", tip.key)), t(&format!("tip-{}-text", tip.key)));
        let mut off = !self.settings.tips;
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("tip")).show(ctx, |ui| {
            ui.set_width(480.0);
            crate::save_dialogs::Head::new(&title).show(ui);
            if let Some(name) = tip.image {
                ui.add_space(4.0);
                CommonMarkViewer::new().show(ui, &mut open.cache, &format!("![]({})", crate::help::image_uri(name)));
            }
            ui.add_space(6.0);
            crate::save_dialogs::notice_text(ui, &text);
            ui.add_space(4.0);
            if ui.link(t("tip-guide")).clicked() {
                choice = Some(Choice::Guide);
            }
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut off, t("tip-off")).on_hover_text(t("tip-off-hint"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("notice-close")).clicked() {
                        choice = Some(Choice::Close);
                    }
                    if ui.button(t("tip-next")).clicked() {
                        choice = Some(Choice::Next);
                    }
                    if ui.button(t("tip-back")).clicked() {
                        choice = Some(Choice::Back);
                    }
                    let n = (i + 1).to_string();
                    let of = TIPS.len().to_string();
                    ui.weak(self.i18n.t_args("tip-count", &[("n", &n), ("of", &of)]));
                });
            });
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
        let len = TIPS.len();
        match choice {
            Some(Choice::Next) => self.tip_shown((i + 1) % len),
            Some(Choice::Back) => self.tip_shown((i + len - 1) % len),
            Some(Choice::Close) => self.tip = None,
            Some(Choice::Guide) => {
                self.tip = None;
                self.open_help(tip.page);
            }
            None => {}
        }
    }

    /// Back or Next: the tip shown is the last one seen.
    fn tip_shown(&mut self, i: usize) {
        self.tip = Some(Open::new(i));
        seen(&mut self.settings, i, today());
        self.settings.save();
    }
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
            assert!(tip.image.is_none_or(|n| crate::help::IMAGES.iter().any(|(name, _)| *name == n)), "{}: picture", tip.key);
            assert!(crate::help::PAGES.iter().any(|p| p.1 == tip.page), "{}: page", tip.key);
        }
    }
}
