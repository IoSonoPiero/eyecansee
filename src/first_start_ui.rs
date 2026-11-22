//! The first-start window (#212), drawn: the welcome page (logo, language, colors) and the
//! add-ons page, with "1 of 2", Back, Next and Skip; on the last page Download and Done (Piero,
//! 21/11). On Windows the desktop icon page (#210, `first_start_desktop.rs`) is between them:
//! "1 of 3". Closing the add-ons page with cards ticked and not downloaded asks first (Piero,
//! 22/11). Which pages it shows, what closing it remembers and when it asks: `first_start.rs`.

use eframe::egui;

use crate::App;
use crate::component::{Addon, Have};
use crate::downloads::Downloads;
use crate::first_start::{After, Answer, Card, Desktop, Page, Plan};
use crate::settings::ThemeChoice;

/// Width of the window.
const WIDTH: f32 = 560.0;
/// Space at the sides of the pages and of the buttons.
const SIDE: i8 = 24;
/// Height of the pages' body, the same on all: Back and Next do not move. The add-ons page
/// shown alone is as tall as its rows.
pub(crate) const BODY: f32 = 440.0;
/// Side of the logo on the welcome page: the size of its texture, so it is sharp.
const LOGO: f32 = 96.0;
/// Air on the welcome page, half above the logo and half under the cards.
const AIR: f32 = 42.0;
/// Height of a choice in a card of the welcome page, and the choices a card shows without
/// scrolling.
const CHOICE: f32 = 30.0;
const CHOICES: f32 = 3.0;
/// Least size of the buttons at the bottom.
const BUTTON: egui::Vec2 = egui::vec2(88.0, 30.0);
/// Width of the question on closing; wider when its buttons need it.
const ASK: f32 = 460.0;

/// The window open.
pub(crate) struct Open {
    plan: Plan,
    page: usize,
    /// The add-ons of its page, where each one is, and whether it is ticked.
    rows: Vec<(Addon, Have, bool)>,
    /// The kinds downloading at the last frame: when one ends, where they are is read again.
    running: Vec<&'static str>,
    /// Closing was asked with ticked cards not downloaded: the question is open over the window.
    asking: bool,
    /// Download and close was answered: these downloads run on the add-ons page, and the window
    /// closes by itself once they have all arrived (`first_start::ended`).
    closing: Option<Vec<&'static str>>,
    /// Where the desktop icon page is (Windows, #210): Back and Next keep its answer.
    desktop: Desktop,
}

/// What a button at the bottom does.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Press {
    Back,
    Next,
    Download,
    Close,
}

impl Open {
    fn new(plan: Plan) -> Self {
        let rows = crate::addons_ui::list().into_iter().filter(|(a, _)| plan.addons.contains(&a.component.kind)).map(|(a, h)| (a, h, false)).collect();
        // Debug only, for the screenshots of a later page: `EYECANSEE_FIRST_START_PAGE=2`.
        let page = std::env::var("EYECANSEE_FIRST_START_PAGE").ok().filter(|_| cfg!(debug_assertions)).and_then(|p| p.parse::<usize>().ok());
        let page = page.map_or(0, |p| p.saturating_sub(1).min(plan.pages.len() - 1));
        // Debug only, for the screenshots of the desktop icon page after an answer:
        // `EYECANSEE_DESKTOP_QUESTION=made|there|no|failed` (any other value: the question).
        let shown = std::env::var("EYECANSEE_DESKTOP_QUESTION").ok().filter(|_| cfg!(debug_assertions));
        let desktop = match shown.as_deref() {
            Some("made") => Desktop::Made("EyeCanSee".into()),
            Some("there") => Desktop::There,
            Some("no") => Desktop::No,
            Some("failed") => Desktop::Failed("debug".into()),
            _ => Desktop::Ask,
        };
        Open { plan, page, rows, running: Vec::new(), asking: false, closing: None, desktop }
    }

    /// The question on closing is open: Enter and Esc are its own (`App::asking`).
    pub(crate) fn asking(&self) -> bool {
        self.asking
    }

    /// The ticked cards that closing now would leave behind (`first_start::waiting`).
    fn waiting(&self, downloads: &Downloads) -> Vec<&'static str> {
        let card = |(a, have, ticked): &(Addon, Have, bool)| {
            let kind = a.component.kind;
            Card { kind, ticked: *ticked, offers: crate::addons_ui::offers(have), started: crate::addons_ui::started(downloads.get(kind)) }
        };
        crate::first_start::waiting(&self.rows.iter().map(card).collect::<Vec<_>>())
    }

    /// Done, Skip or Esc: true when the window closes now. With ticked cards not downloaded it
    /// stays, and the question opens, from either page (Piero, 22/11: Skip on the welcome page too,
    /// after Back); its Back leaves the page as it was.
    fn leave(&mut self, downloads: &Downloads) -> bool {
        self.asking = !self.waiting(downloads).is_empty();
        !self.asking
    }
}

impl App {
    /// At the start, before the tip of the day: true when the window opens (then no tip today).
    pub(crate) fn first_start_at_start(&mut self, opened_with_address: bool) -> bool {
        let offers: Vec<_> = crate::addons_ui::list().iter().map(|(a, h)| (a.component.kind, crate::addons_ui::offers(h))).collect();
        let before = self.settings.clone();
        let plan = crate::first_start::at_start(&mut self.settings, &offers, crate::desktop_icon::ask_at_start, opened_with_address, crate::tips::today());
        if self.settings != before {
            self.settings.save();
        }
        let Some(plan) = plan else { return false };
        log!("first start: pages {:?}, add-ons {:?}", plan.pages, plan.addons);
        self.first_start = Some(Open::new(plan));
        true
    }

    /// Before the pages read the keys: Esc closes the window, as Skip and Done. With the question
    /// on closing open Esc is the question's (Back): it comes as `key_answer`, not here.
    pub(crate) fn first_start_esc(&mut self, ctx: &egui::Context) {
        if self.first_start.as_ref().is_some_and(|open| !open.asking) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            log!("first start: Esc");
            self.first_start_leave();
        }
    }

    /// Done, Skip or Esc: the window closes, and remembers (`first_start::closed`). From the
    /// add-ons page with ticked cards not downloaded the question comes first (`first_start_ask`).
    fn first_start_leave(&mut self) {
        let Some(open) = &mut self.first_start else { return };
        if !open.leave(&self.downloads) {
            log!("first start: closing asked with {:?} ticked and not downloaded", open.waiting(&self.downloads));
            return;
        }
        log!("first start: closed");
        crate::first_start::closed(&mut self.settings, &open.plan);
        self.settings.save();
        self.first_start = None;
    }

    /// The question when closing would leave ticked cards not downloaded: Download and close
    /// (Enter too), Close without downloading, or Back to the page with its ticks (Esc too). The
    /// downloads start as Download starts them, and go on after the window is closed.
    fn first_start_ask(&mut self, ctx: &egui::Context) {
        let Some(open) = self.first_start.as_mut().filter(|open| open.asking) else { return };
        let waiting = open.waiting(&self.downloads);
        let t = |id| self.i18n.t(id);
        let question = Question {
            icons: open.rows.iter().filter(|r| waiting.contains(&r.0.component.kind)).map(|r| r.0.icon).collect(),
            title: t("first-start-addons"),
            text: self.i18n.t_args("first-start-leave", &[("n", &waiting.len().to_string())]),
            buttons: [(t("first-start-back"), Answer::Back), (t("first-start-leave-close"), Answer::Close), (t("first-start-leave-download"), Answer::Download)],
        };
        // Enter is the main button, Esc is Back.
        let keys = crate::confirm::answer(ctx, &mut self.key_answer, ASK_ID).map(|enter| if enter { Answer::Download } else { Answer::Back });
        let Some(answer) = ask(ctx, &question).or(keys) else { return };
        open.asking = false;
        log!("first start: {answer:?} answered");
        match crate::first_start::answered(&mut self.settings, &open.plan, waiting, answer) {
            After::Stay => {}
            After::Closed => {
                self.settings.save();
                self.first_start = None;
            }
            // The window stays, on the add-ons page with their bars, and closes once they arrive.
            After::Downloading(kinds) => {
                open.page = open.plan.pages.iter().position(|p| *p == Page::Addons).unwrap_or(open.page);
                for (a, ..) in open.rows.iter().filter(|r| kinds.contains(&r.0.component.kind)) {
                    if self.downloads.start(a.component, ctx) {
                        log!("first start: downloading {} before closing", a.component.kind);
                    }
                }
                open.closing = Some(kinds);
            }
        }
    }

    /// After Download and close: all arrived, the window closes by itself as Done closes it; one
    /// failed, it stays open with the error, and Done closes it.
    fn first_start_closing(&mut self) {
        let Some(open) = &mut self.first_start else { return };
        let Some(kinds) = &open.closing else { return };
        let states: Vec<_> = kinds.iter().map(|k| crate::addons_ui::download(self.downloads.get(k))).collect();
        match crate::first_start::ended(&states) {
            None => {}
            Some(false) => {
                log!("first start: a download failed, the window stays");
                open.closing = None;
            }
            Some(true) => {
                log!("first start: downloads arrived, closed");
                crate::first_start::closed(&mut self.settings, &open.plan);
                self.settings.save();
                self.first_start = None;
            }
        }
    }

    /// The window, while open.
    pub(crate) fn first_start_window(&mut self, ctx: &egui::Context) {
        if self.first_start.is_none() {
            return;
        }
        self.first_start_reread();
        let before = self.settings.clone();
        let press = self.first_start_draw(ctx);
        // Language and colors change at once, the window too, and are saved as in Options.
        self.options_changed(ctx, &before);
        match press {
            Some(Press::Back) => self.first_start_turn(-1),
            Some(Press::Next) => self.first_start_turn(1),
            Some(Press::Download) => self.first_start_download(ctx),
            Some(Press::Close) => self.first_start_leave(),
            None => {}
        }
        self.first_start_ask(ctx);
        self.first_start_closing();
    }

    fn first_start_turn(&mut self, by: isize) {
        if let Some(open) = &mut self.first_start {
            open.page = open.page.saturating_add_signed(by).min(open.plan.pages.len() - 1);
        }
    }

    /// Download: the ticked ones start, each in its place (`App::downloads`), as the red buttons
    /// would start them; the parts of EyeCanSee waiting for one see it arrive as they do today.
    fn first_start_download(&mut self, ctx: &egui::Context) {
        let Some(open) = &mut self.first_start else { return };
        // The ticks stay: a download that fails leaves its card ticked, for Download again.
        for (a, ..) in open.rows.iter().filter(|r| r.2) {
            if self.downloads.start(a.component, ctx) {
                log!("first start: downloading {}", a.component.kind);
            }
        }
    }

    /// A download of the page ended: where the add-ons are is read again.
    fn first_start_reread(&mut self) {
        let Some(open) = &mut self.first_start else { return };
        let running: Vec<_> = open.rows.iter().map(|r| r.0.component.kind).filter(|k| crate::downloads::running(self.downloads.get(k))).collect();
        if running.len() < open.running.len() {
            for (a, have, _) in &mut open.rows {
                *have = crate::component::have(a);
            }
        }
        open.running = running;
    }

    /// The page shown and the buttons; the button pressed.
    fn first_start_draw(&mut self, ctx: &egui::Context) -> Option<Press> {
        let icon = self.about_icon.get_or_insert_with(|| {
            let img = crate::decode::app_icon(96).unwrap_or_default();
            ctx.load_texture("about-icon", crate::decode::to_color_image(img.into()), egui::TextureOptions::LINEAR)
        });
        let logo = icon.id();
        let open = self.first_start.as_mut()?;
        let (i18n, languages, s, downloads) = (&self.i18n, &self.languages, &mut self.settings, &self.downloads);
        let (page, paged) = (open.plan.pages[open.page], open.plan.pages.len() > 1);
        let mut press = None;
        // No margin of the window's: the band of the buttons reaches its edges.
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(0);
        egui::Modal::new(egui::Id::unique("first-start")).frame(frame).show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new().inner_margin(egui::Margin::symmetric(SIDE, 0)).show(ui, |ui| {
                if paged {
                    ui.set_height(BODY);
                }
                match page {
                    Page::Welcome => welcome(ui, logo, s, i18n, languages),
                    Page::Desktop => crate::first_start_desktop::page(ui, logo, &mut open.desktop, i18n),
                    Page::Addons => addons(ui, &mut open.rows, downloads, i18n, paged),
                }
            });
            press = footer(ui, open, i18n);
        });
        press
    }
}

/// The title of a page: bold and larger than the windows' headings, centred by the caller.
pub(crate) fn title(text: String) -> egui::RichText {
    egui::RichText::new(text).font(egui::FontId::new(22.0, egui::FontFamily::Name(crate::BOLD.into())))
}

/// Logo and welcome, then language and colors in two cards side by side.
fn welcome(ui: &mut egui::Ui, logo: egui::TextureId, s: &mut crate::settings::Settings, i18n: &crate::i18n::I18n, languages: &[(&'static str, String)]) {
    ui.vertical_centered(|ui| {
        ui.add_space(20.0 + AIR / 2.0);
        ui.image((logo, egui::vec2(LOGO, LOGO)));
        ui.add_space(16.0);
        ui.label(title(i18n.t("first-start-welcome")));
        ui.add_space(6.0);
        ui.label(i18n.t("first-start-welcome-text"));
    });
    ui.add_space(22.0);
    let gap = 12.0;
    let width = (ui.available_width() - gap) / 2.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        card(ui, width, &i18n.t("opt-language"), |ui| {
            for (value, name) in crate::app_dialogs::language_choices(i18n, languages) {
                if choice(ui, s.language == value, &name, None) {
                    s.language = value;
                }
            }
        });
        card(ui, width, &i18n.t("opt-theme"), |ui| {
            for (value, name) in crate::app_dialogs::theme_choices(i18n) {
                if choice(ui, s.theme == value, &name, Some(value)) {
                    s.theme = value;
                }
            }
        });
    });
    // The last line in the middle of what is left.
    ui.centered_and_justified(|ui| ui.label(egui::RichText::new(i18n.t("first-start-welcome-later")).weak()));
}

/// A card of the welcome page, `width` wide: its name in bold and its choices, which scroll when
/// they are more than `CHOICES` (a language more).
fn card(ui: &mut egui::Ui, width: f32, name: &str, choices: impl FnOnce(&mut egui::Ui)) {
    let frame = crate::theme::card(ui.visuals());
    frame.show(ui, |ui| {
        ui.set_width(width - frame.total_margin().sum().x);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(egui::RichText::new(name).family(egui::FontFamily::Name(crate::BOLD.into())));
            ui.add_space(6.0);
            let height = CHOICES * CHOICE + (CHOICES - 1.0) * 2.0;
            egui::ScrollArea::vertical().id_salt(name).max_height(height).auto_shrink([false, false]).show(ui, choices);
        });
    });
}

/// One choice of a card, the whole row to click: the round mark, its name and, for the colors,
/// their picture. True when it is clicked.
fn choice(ui: &mut egui::Ui, selected: bool, name: &str, sample: Option<ThemeChoice>) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), CHOICE), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, name));
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let v = ui.visuals();
    let (accent, painter) = (crate::theme::accent(v), ui.painter());
    if selected {
        painter.rect(rect, 6, crate::theme::tile_selected(v), egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    } else if response.hovered() || response.has_focus() {
        painter.rect_filled(rect, 6, v.widgets.hovered.weak_bg_fill);
    }
    let mark = egui::pos2(rect.left() + 17.0, rect.center().y);
    let ring = if selected { accent } else { crate::theme::tile(v).1 };
    painter.circle(mark, 7.0, v.extreme_bg_color, egui::Stroke::new(1.5, ring));
    if selected {
        painter.circle_filled(mark, 3.5, accent);
    }
    let picture = egui::Rect::from_center_size(egui::pos2(rect.right() - 28.0, rect.center().y), egui::vec2(34.0, 22.0));
    let room = if sample.is_some() { picture.left() - 8.0 } else { rect.right() - 8.0 } - (mark.x + 17.0);
    let text = egui::RichText::new(name).color(if selected { v.strong_text_color() } else { v.text_color() });
    let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Truncate), room, egui::TextStyle::Body);
    painter.galley(egui::pos2(mark.x + 17.0, rect.center().y - galley.size().y / 2.0), galley, v.text_color());
    match sample {
        Some(ThemeChoice::System) => {
            theme_sample(painter, picture, false);
            theme_sample(&painter.with_clip_rect(picture.with_min_x(picture.center().x)), picture, true);
        }
        Some(theme) => theme_sample(painter, picture, theme == ThemeChoice::Dark),
        None => {}
    }
    response.clicked()
}

/// EyeCanSee in small with a theme's colours: the grid, the panel at its left and the blue bar at
/// the bottom.
fn theme_sample(painter: &egui::Painter, rect: egui::Rect, dark: bool) {
    let [grid, panel, bar, edge] = crate::theme::sample(dark);
    let r = 4;
    painter.rect_filled(rect, r, grid);
    let left = rect.with_max_x(rect.left() + 11.0);
    painter.rect_filled(left, egui::CornerRadius { nw: r, sw: r, ne: 0, se: 0 }, panel);
    let bottom = rect.with_min_y(rect.bottom() - 5.0);
    painter.rect_filled(bottom, egui::CornerRadius { sw: r, se: r, nw: 0, ne: 0 }, bar);
    painter.rect_stroke(rect, r, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
}

/// The add-ons, one card each, with a box to tick; `paged`: after the welcome page, as tall as
/// that one, the last lines at the bottom.
fn addons(ui: &mut egui::Ui, rows: &mut [(Addon, Have, bool)], downloads: &crate::downloads::Downloads, i18n: &crate::i18n::I18n, paged: bool) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(title(i18n.t("first-start-addons")));
        ui.add_space(6.0);
        ui.add(egui::Label::new(i18n.t("first-start-addons-text")).wrap());
    });
    ui.add_space(18.0);
    let later = egui::Label::new(egui::RichText::new(i18n.t("first-start-addons-later")).weak()).wrap();
    let mut list = |ui: &mut egui::Ui| {
        egui::ScrollArea::vertical().max_height(ui.available_height().min(BODY)).auto_shrink([false, true]).show(ui, |ui| {
            for (i, (a, have, ticked)) in rows.iter_mut().enumerate() {
                if i > 0 {
                    ui.add_space(8.0);
                }
                let slot = downloads.get(a.component.kind);
                crate::addons_ui::row(ui, (a, have), slot, i18n, crate::addons_ui::Control::Tick(ticked));
            }
        });
    };
    if paged {
        // From the bottom: the last lines, then the rows in what is left above them.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(16.0);
            ui.add(later);
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), list);
        });
    } else {
        list(ui);
        ui.add_space(14.0);
        ui.vertical_centered(|ui| ui.add(later));
        ui.add_space(16.0);
    }
}

/// A button at the bottom: its text, what it does, whether it can be pressed and whether it is
/// the one to press now (drawn solid).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Button {
    text: &'static str,
    press: Press,
    enabled: bool,
    main: bool,
    /// Why it cannot be pressed, said when the pointer is on it greyed.
    hint: Option<&'static str>,
}

/// The buttons of page `n` of `of`, from the left: Skip and Next, with Back between them on a
/// page in the middle, or Back, Download and Done on the last page (Piero, 21/11: Skip there
/// would be Done).
/// The main one is Next, then Done; with an add-on ticked it is Download (a choice of the
/// redesign, not yet seen by Piero), and Done then asks before leaving the ticks behind
/// (`Open::leave`). `waits`: the page wants an answer first (`first_start::waits`), so Next is
/// greyed, with this hint; Back and Skip stay.
fn buttons(n: usize, of: usize, ticked: bool, waits: Option<&'static str>) -> Vec<Button> {
    let button = |text, press, enabled, main| Button { text, press, enabled, main, hint: None };
    let mut all = Vec::new();
    if n < of {
        all.push(button("first-start-skip", Press::Close, true, false));
    }
    if n > 1 {
        all.push(button("first-start-back", Press::Back, true, false));
    }
    if n < of {
        all.push(Button { hint: waits, ..button("first-start-next", Press::Next, waits.is_none(), true) });
    } else {
        all.push(button("first-start-download", Press::Download, ticked, ticked));
        all.push(button("first-start-done", Press::Close, true, !ticked));
    }
    all
}

/// The band at the bottom of a window of this family, for its buttons: the panels' colour to the
/// window's edges, a line above it; what `row` draws, on one row.
fn band(ui: &mut egui::Ui, row: impl FnOnce(&mut egui::Ui)) {
    let v = ui.visuals().clone();
    let round = v.window_corner_radius;
    let frame = egui::Frame::new().fill(v.panel_fill).corner_radius(egui::CornerRadius { nw: 0, ne: 0, ..round }).inner_margin(egui::Margin::symmetric(SIDE, 12));
    let rect = frame.show(ui, |ui| ui.horizontal(row)).response.rect;
    ui.painter().hline(rect.x_range(), rect.top(), v.widgets.noninteractive.bg_stroke);
}

/// The band at the bottom: the pages' dots and "1 of 2" on the left, the buttons on the right.
fn footer(ui: &mut egui::Ui, open: &Open, i18n: &crate::i18n::I18n) -> Option<Press> {
    let (n, of) = (open.page + 1, open.plan.pages.len());
    let mut press = None;
    band(ui, |ui| {
        if of > 1 {
            dots(ui, n, of);
            ui.weak(i18n.t_args("first-start-count", &[("n", &n.to_string()), ("of", &of.to_string())]));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let waits = crate::first_start::waits(open.plan.pages[open.page], &open.desktop);
            for b in buttons(n, of, open.rows.iter().any(|r| r.2), waits).into_iter().rev() {
                let text = i18n.t(b.text);
                let button = if b.main { crate::theme::main_button(ui, text) } else { egui::Button::new(text) };
                let mut response = ui.add_enabled(b.enabled, button.min_size(BUTTON));
                if let Some(hint) = b.hint {
                    response = response.on_disabled_hover_text(i18n.t(hint));
                }
                if response.clicked() {
                    press = Some(b.press);
                }
            }
        });
    });
    press
}

/// The modal of the question on closing: Enter and Esc reach it by this name (`confirm::answer`).
const ASK_ID: &str = "first-start-leave";

/// What the question on closing shows: the icons of the add-ons chosen and not downloaded, its
/// title and sentence, and its buttons from the left, the last one the main one.
struct Question {
    icons: Vec<crate::icons::Icon>,
    title: String,
    text: String,
    buttons: [(String, Answer); 3],
}

/// The question on closing, over the window and of its family: the icons of the add-ons chosen,
/// the title and the sentence in the middle, and the band of the buttons, Back at its left, the
/// two that close at its right. As wide as its buttons need in any language: no text is cut.
/// The button pressed.
fn ask(ctx: &egui::Context, q: &Question) -> Option<Answer> {
    let mut answer = None;
    let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(0);
    egui::Modal::new(egui::Id::unique(ASK_ID)).frame(frame).show(ctx, |ui| {
        // Air at the sides of the long texts of its buttons, and at least some between Back and
        // the other two.
        ui.spacing_mut().button_padding.x = 12.0;
        let buttons: f32 = q.buttons.iter().map(|b| crate::theme::hint_width(ui, &b.0).max(BUTTON.x)).sum();
        ui.set_width(ASK.max(buttons + 2.0 * ui.spacing().item_spacing.x + 24.0 + 2.0 * f32::from(SIDE)));
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::Frame::new().inner_margin(egui::Margin::symmetric(SIDE, 0)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(26.0);
                let (side, gap) = (40.0, 10.0);
                let (row, _) = ui.allocate_exact_size(egui::vec2(q.icons.len() as f32 * (side + gap) - gap, side), egui::Sense::hover());
                for (i, icon) in q.icons.iter().enumerate() {
                    let at = row.min + egui::vec2(i as f32 * (side + gap), 0.0);
                    crate::icons::paint(ui.painter(), *icon, egui::Rect::from_min_size(at, egui::Vec2::splat(side)));
                }
                ui.add_space(16.0);
                ui.label(title(q.title.clone()).size(18.0));
                ui.add_space(8.0);
                ui.add(egui::Label::new(&q.text).wrap());
                ui.add_space(26.0);
            });
        });
        let [back, close, main] = &q.buttons;
        band(ui, |ui| {
            if ui.add(egui::Button::new(&back.0).min_size(BUTTON)).clicked() {
                answer = Some(back.1);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(crate::theme::main_button(ui, main.0.clone()).min_size(BUTTON)).clicked() {
                    answer = Some(main.1);
                }
                if ui.add(egui::Button::new(&close.0).min_size(BUTTON)).clicked() {
                    answer = Some(close.1);
                }
            });
        });
    });
    answer
}

/// One dot a page, the one shown in the accent.
fn dots(ui: &mut egui::Ui, n: usize, of: usize) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(of as f32 * 13.0 - 6.0, BUTTON.y), egui::Sense::hover());
    let v = ui.visuals();
    for i in 0..of {
        let colour = if i + 1 == n { crate::theme::accent(v) } else { crate::theme::tile(v).1.gamma_multiply(0.5) };
        ui.painter().circle_filled(egui::pos2(rect.left() + 3.5 + i as f32 * 13.0, rect.center().y), 3.5, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(buttons: &[Button]) -> Vec<(&'static str, bool, bool)> {
        buttons.iter().map(|b| (b.text.trim_start_matches("first-start-"), b.enabled, b.main)).collect()
    }

    /// The choices of the welcome page are rows drawn here, not egui's round buttons: a click on
    /// a row takes it, for the colors and for the language, "Same as the system" too.
    #[test]
    fn a_click_on_a_choice_takes_it() {
        use crate::addons_ui::tests::{click, headless};
        let (ctx, area) = headless();
        let i18n = crate::i18n::I18n::new("en");
        // The middle of row `i`, with egui's space between two rows.
        let row = |i: usize| area.min + egui::vec2(60.0, i as f32 * (CHOICE + ctx.global_style().spacing.item_spacing.y) + CHOICE / 2.0);
        let mut theme = ThemeChoice::System;
        for (i, wanted) in [(2, ThemeChoice::Dark), (1, ThemeChoice::Light), (0, ThemeChoice::System)] {
            click(&ctx, row(i), |ui| {
                for (value, name) in crate::app_dialogs::theme_choices(&i18n) {
                    if choice(ui, theme == value, &name, Some(value)) {
                        theme = value;
                    }
                }
            });
            assert_eq!(theme, wanted);
        }
        let languages = [("en", "English".to_string()), ("it", "Italiano".to_string())];
        let mut language = None;
        for (i, wanted) in [(2, Some("it")), (1, Some("en")), (0, None)] {
            click(&ctx, row(i), |ui| {
                for (value, name) in crate::app_dialogs::language_choices(&i18n, &languages) {
                    if choice(ui, language == value, &name, None) {
                        language = value;
                    }
                }
            });
            assert_eq!(language.as_deref(), wanted);
        }
    }

    /// The sentence of the question says how many add-ons were chosen, one or more, in both
    /// languages.
    #[test]
    fn the_question_counts_the_addons() {
        let say = |lang: &str, n: usize| crate::i18n::I18n::new(lang).t_args("first-start-leave", &[("n", &n.to_string())]);
        assert_eq!(say("en", 1), "You chose 1 add-on but did not download it.");
        assert_eq!(say("en", 3), "You chose 3 add-ons but did not download them.");
        assert_eq!(say("it", 1), "Hai scelto 1 componente ma non l'hai scaricato.");
        assert_eq!(say("it", 2), "Hai scelto 2 componenti ma non li hai scaricati.");
    }

    /// Enter and Esc are the question's while it is open over the window, not the window's
    /// (`confirm::answer` gives them to the modal on top).
    #[test]
    fn the_keys_go_to_the_question_on_top() {
        use crate::addons_ui::tests::{click, headless};
        let (ctx, area) = headless();
        let button = |text: &str, answer| (text.to_string(), answer);
        let question = Question {
            icons: vec![crate::icons::Icon::Folder],
            title: "Add-ons".into(),
            text: "You chose 1 add-on but did not download it.".into(),
            buttons: [button("Back", Answer::Back), button("Close without downloading", Answer::Close), button("Download and close", Answer::Download)],
        };
        let mut pressed = None;
        // A click away from both: no button is pressed.
        click(&ctx, area.min + egui::vec2(2.0, 2.0), |ui| {
            egui::Modal::new(egui::Id::unique("first-start")).show(ui.ctx(), |ui| ui.label("the window"));
            pressed = pressed.or(ask(ui.ctx(), &question));
        });
        assert_eq!(pressed, None);
        assert_eq!(crate::confirm::answer(&ctx, &mut Some(false), "first-start"), None, "not the window under it");
        assert_eq!(crate::confirm::answer(&ctx, &mut Some(false), ASK_ID), Some(false), "Esc");
        assert_eq!(crate::confirm::answer(&ctx, &mut Some(true), ASK_ID), Some(true), "Enter");
    }

    /// Done, Skip and Esc close at once with nothing ticked; with a card ticked and not
    /// downloaded the add-ons page asks first, the welcome page closes as before. A tick whose
    /// download is under way, or on an add-on already here, does not ask; one whose download
    /// failed does.
    #[test]
    fn closing_asks_with_ticks_not_downloaded() {
        let rows: Vec<_> = crate::component::addons().into_iter().map(|a| (a, Have::Missing, false)).collect();
        let Some(kind) = rows.first().map(|r| r.0.component.kind) else { return };
        let plan = Plan { pages: vec![Page::Welcome, Page::Addons], addons: rows.iter().map(|r| r.0.component.kind).collect() };
        let mut open = Open { plan, page: 1, rows, running: Vec::new(), asking: false, closing: None, desktop: Desktop::Ask };
        let mut downloads = Downloads::default();
        assert!(open.leave(&downloads), "nothing ticked");
        open.rows[0].2 = true;
        assert!(!open.leave(&downloads) && open.asking(), "a card ticked, not downloaded");
        assert_eq!(open.waiting(&downloads), [kind]);
        open.page = 0;
        assert!(!open.leave(&downloads) && open.asking(), "Skip on the welcome page asks too");
        assert_eq!(open.page, 0, "and its Back goes back there");
        open.page = 1;
        open.rows[0].1 = Have::Downloaded;
        assert!(open.leave(&downloads), "already downloaded");
        open.rows[0].1 = Have::OnSystem("1".into());
        assert!(open.leave(&downloads), "already on the system");
        open.rows[0].1 = Have::Missing;
        let download = std::sync::Arc::new(crate::component::Download::default());
        *downloads.slot(kind).unwrap() = Some(download.clone());
        assert!(open.leave(&downloads), "its download is under way");
        *download.result.lock().unwrap() = Some(Ok(()));
        assert!(open.leave(&downloads), "its download has arrived");
        *download.result.lock().unwrap() = Some(Err("x".into()));
        assert!(open.leave(&downloads), "its download failed: the card stays ticked and Done closes (Piero, 22/11)");
    }

    /// Skip and Next on the first page; Back, Download and Done on the last, without Back when
    /// it is the only one; Skip, Back and Next on one in the middle, Next greyed while its page
    /// waits for an answer. One button is the main one: Download once an add-on is ticked.
    #[test]
    fn buttons_of_each_page() {
        assert_eq!(named(&buttons(1, 2, false, None)), [("skip", true, false), ("next", true, true)]);
        assert_eq!(named(&buttons(2, 2, false, None)), [("back", true, false), ("download", false, false), ("done", true, true)]);
        assert_eq!(named(&buttons(2, 2, true, None)), [("back", true, false), ("download", true, true), ("done", true, false)]);
        assert_eq!(named(&buttons(1, 1, false, None)), [("download", false, false), ("done", true, true)]);
        // Three pages (Windows, the desktop icon in the middle): Skip, Back and Next there.
        assert_eq!(named(&buttons(1, 3, false, None)), [("skip", true, false), ("next", true, true)]);
        assert_eq!(named(&buttons(2, 3, true, None)), [("skip", true, false), ("back", true, false), ("next", true, true)]);
        assert_eq!(named(&buttons(3, 3, true, None)), [("back", true, false), ("download", true, true), ("done", true, false)]);
        // A page that waits for an answer (the desktop icon): Next greyed, with why; Skip and Back
        // as they are. The last page has no Next: nothing changes there.
        let waiting = buttons(2, 3, false, Some("desktop-icon-choose"));
        assert_eq!(named(&waiting), [("skip", true, false), ("back", true, false), ("next", false, true)]);
        assert_eq!(waiting.iter().map(|b| b.hint).collect::<Vec<_>>(), [None, None, Some("desktop-icon-choose")]);
        assert_eq!(buttons(3, 3, false, Some("desktop-icon-choose")), buttons(3, 3, false, None));
        assert!(buttons(2, 3, false, None).iter().all(|b| b.hint.is_none()));
        for (n, of, ticked) in [(1, 2, false), (2, 2, false), (2, 2, true), (1, 1, true), (2, 3, false), (3, 3, false)] {
            assert_eq!(buttons(n, of, ticked, None).iter().filter(|b| b.main).count(), 1);
        }
    }
}
