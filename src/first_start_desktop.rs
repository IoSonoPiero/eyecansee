//! The desktop icon page of the first-start window (Windows, #210; Piero, 22/11: a page of its
//! own, not a row of the welcome page): its title, a picture of a desktop with the EyeCanSee icon
//! on it, and two large choices, Yes and No. Yes makes the icon at once and the picture shows it
//! there; No takes it off the picture. When the page is shown and what an answer does:
//! `first_start.rs`.

use eframe::egui;

use crate::first_start::Desktop;
use crate::i18n::I18n;

/// Height of the picture: the screen and its stand.
const PICTURE: f32 = 202.0;
/// The screen of the picture, with its frame.
const SCREEN: egui::Vec2 = egui::vec2(320.0, 180.0);
/// Side of the EyeCanSee icon on the picture: half of its texture, so it is sharp.
const ICON: f32 = 48.0;
/// Height of a choice.
const ANSWER: f32 = 52.0;

/// The EyeCanSee icon on the picture.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Icon {
    /// Not there yet: faint, in a dashed frame, where Yes would put it.
    Offered,
    /// On the desktop: solid, with a green mark.
    Here,
    /// No was answered: the desktop without it.
    Gone,
}

/// The line under the choices.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Line<'a> {
    /// Faint, as the last line of the other pages.
    Weak(&'static str),
    Plain(&'static str),
    /// With the triangle, never red; the system's reason on hover.
    Failed(&'a str),
}

/// What the page shows for where the question is.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Look<'a> {
    icon: Icon,
    /// The name under the icon: the one it was made with, or the one it would take.
    name: &'a str,
    /// The choice marked, if any: Yes (true) or No.
    marked: Option<bool>,
    /// The choices can still be clicked: not once the icon is on the desktop.
    open: bool,
    line: Line<'a>,
}

/// `base`: the name a new icon takes (`desktop_icon::base_name`).
fn look<'a>(d: &'a Desktop, base: &'a str) -> Look<'a> {
    let later = Line::Weak("desktop-icon-later");
    match d {
        Desktop::Ask => Look { icon: Icon::Offered, name: base, marked: None, open: true, line: later },
        Desktop::No => Look { icon: Icon::Gone, name: base, marked: Some(false), open: true, line: later },
        Desktop::Made(name) => Look { icon: Icon::Here, name, marked: Some(true), open: false, line: Line::Plain("desktop-icon-done") },
        Desktop::There => Look { icon: Icon::Here, name: base, marked: Some(true), open: false, line: Line::Plain("desktop-icon-there") },
        // Nothing marked: the icon is not there, and Yes can be pressed again.
        Desktop::Failed(why) => Look { icon: Icon::Offered, name: base, marked: None, open: true, line: Line::Failed(why) },
    }
}

/// The page: title and question, the picture, Yes and No side by side, and the line about what
/// was answered in the middle of what is left. `logo`: the app's icon, 96 pixels.
pub(crate) fn page(ui: &mut egui::Ui, logo: egui::TextureId, d: &mut Desktop, i18n: &I18n) {
    page_with(ui, logo, d, i18n, crate::desktop_icon::create);
}

/// `page`, with what Yes does given: the tests make no icon.
fn page_with(ui: &mut egui::Ui, logo: egui::TextureId, d: &mut Desktop, i18n: &I18n, create: impl FnOnce() -> Result<crate::desktop_icon::Made, String>) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(crate::first_start_ui::title(i18n.t("desktop-icon-title")));
        ui.add_space(6.0);
        ui.add(egui::Label::new(i18n.t("desktop-icon-question")).wrap());
    });
    ui.add_space(16.0);
    let base = crate::desktop_icon::me().map_or("EyeCanSee", |me| crate::desktop_icon::base_name(me.copy));
    let shown = look(d, base);
    picture(ui, logo, shown.icon, shown.name);
    ui.add_space(16.0);
    let gap = 12.0;
    let width = (ui.available_width() - gap) / 2.0;
    let mut pressed = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (yes, id) in [(true, "desktop-icon-yes"), (false, "desktop-icon-no")] {
            if answer(ui, width, &i18n.t(id), shown.marked == Some(yes), shown.open) {
                pressed = Some(yes);
            }
        }
    });
    line(ui, shown.line, i18n);
    if let Some(yes) = pressed {
        *d = crate::first_start::chosen(std::mem::replace(d, Desktop::Ask), yes, create);
        log!("first start: desktop icon {}: {d:?}", if yes { "Yes" } else { "No" });
    }
}

/// One choice, a card `width` wide, all of it to click: the round mark and its word in the
/// middle. Marked it is the chosen card of the add-ons page. True when it is clicked.
fn answer(ui: &mut egui::Ui, width: f32, text: &str, marked: bool, open: bool) -> bool {
    let sense = if open { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, ANSWER), sense);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, open, marked, text));
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let v = ui.visuals();
    let (accent, painter) = (crate::theme::accent(v), ui.painter());
    let over = open && (response.hovered() || response.has_focus());
    let frame = if marked { crate::theme::card_chosen(v) } else { crate::theme::card(v) };
    let edge = if over && !marked { v.widgets.hovered.bg_stroke } else { frame.stroke };
    painter.rect(rect, frame.corner_radius, frame.fill, edge, egui::StrokeKind::Inside);
    // Faint when it can no longer be chosen: the other one was, and stays.
    let faint = !open && !marked;
    let ink = if faint { v.weak_text_color() } else { v.strong_text_color() };
    let font = egui::FontId::new(16.0, egui::FontFamily::Name(crate::BOLD.into()));
    let galley = painter.layout_no_wrap(text.to_owned(), font, ink);
    let (mark, space) = (16.0, 10.0);
    let left = rect.center().x - (mark + space + galley.size().x) / 2.0;
    let centre = egui::pos2(left + mark / 2.0, rect.center().y);
    let ring = if marked { accent } else { crate::theme::tile(v).1.gamma_multiply(if faint { 0.5 } else { 1.0 }) };
    painter.circle(centre, mark / 2.0, v.extreme_bg_color, egui::Stroke::new(1.5, ring));
    if marked {
        painter.circle_filled(centre, 4.0, accent);
    }
    painter.galley(egui::pos2(left + mark + space, rect.center().y - galley.size().y / 2.0), galley, ink);
    response.clicked()
}

/// The line under the choices, in the middle of what is left of the page.
fn line(ui: &mut egui::Ui, line: Line, i18n: &I18n) {
    match line {
        Line::Weak(id) => {
            ui.centered_and_justified(|ui| ui.label(egui::RichText::new(i18n.t(id)).weak()));
        }
        Line::Plain(id) => {
            ui.centered_and_justified(|ui| ui.label(i18n.t(id)));
        }
        Line::Failed(why) => {
            // `error_bar::inline` starts at the left: its width, known after it is drawn, puts it
            // in the middle from the next frame; the first one draws it unseen.
            let id = egui::Id::unique("first-start-desktop-failed");
            let known: Option<f32> = ui.data(|data| data.get_temp(id));
            let room = ui.available_size();
            ui.add_space(((room.y - 22.0) / 2.0).max(0.0));
            ui.horizontal(|ui| {
                if known.is_none() {
                    ui.set_invisible();
                }
                ui.add_space(((room.x - known.unwrap_or(0.0)) / 2.0).max(0.0));
                let width = crate::error_bar::inline(ui, &i18n.t("desktop-icon-failed")).on_hover_text(why).rect.width();
                if known != Some(width) {
                    ui.data_mut(|data| data.insert_temp(id, width));
                    ui.ctx().request_repaint();
                }
            });
        }
    }
}

/// The colours of the picture, the same in both themes but for the screen's frame: the frame and
/// the stand, the wallpaper and its two lighter shapes, the taskbar, the green mark.
struct Colours {
    frame: egui::Color32,
    wallpaper: egui::Color32,
    shapes: [egui::Color32; 2],
    taskbar: egui::Color32,
    good: egui::Color32,
}

fn colours(v: &egui::Visuals) -> Colours {
    let rgb = egui::Color32::from_rgb;
    Colours {
        // Against the window: dark on white, lighter on the dark one.
        frame: if v.dark_mode { rgb(92, 98, 110) } else { rgb(52, 58, 70) },
        wallpaper: rgb(58, 122, 214),
        shapes: [rgb(96, 156, 236), rgb(134, 186, 246)],
        taskbar: rgb(30, 66, 128),
        good: rgb(30, 150, 70),
    }
}

/// A screen on its stand with a desktop on it: a wallpaper, the taskbar, two icons of other
/// programs as plain squares, and in the middle the EyeCanSee icon with its name under it.
fn picture(ui: &mut egui::Ui, logo: egui::TextureId, icon: Icon, name: &str) {
    use egui::emath::GuiRounding as _;
    let (room, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), PICTURE), egui::Sense::hover());
    if !ui.is_rect_visible(room) {
        return;
    }
    let c = colours(ui.visuals());
    let pixels = ui.pixels_per_point();
    let outer = egui::Rect::from_min_size(egui::pos2(room.center().x - SCREEN.x / 2.0, room.top()), SCREEN).round_to_pixels(pixels);
    let painter = ui.painter();
    // The stand first, under the frame.
    let neck = egui::Rect::from_min_size(egui::pos2(outer.center().x - 20.0, outer.bottom() - 2.0), egui::vec2(40.0, 16.0)).round_to_pixels(pixels);
    let foot = egui::Rect::from_min_size(egui::pos2(outer.center().x - 58.0, neck.bottom() - 1.0), egui::vec2(116.0, 7.0)).round_to_pixels(pixels);
    painter.rect_filled(neck, 0, c.frame.gamma_multiply(0.8));
    painter.rect_filled(foot, 3, c.frame);
    painter.rect_filled(outer, 9, c.frame);
    let screen = outer.shrink(6.0);
    painter.rect_filled(screen, 3, c.wallpaper);
    let taskbar = screen.with_min_y(screen.bottom() - 15.0);
    let on_screen = painter.with_clip_rect(screen.with_max_y(taskbar.top()));
    on_screen.circle_filled(screen.right_bottom() + egui::vec2(-36.0, 30.0), 120.0, c.shapes[0]);
    on_screen.circle_filled(screen.right_bottom() + egui::vec2(10.0, 44.0), 84.0, c.shapes[1]);
    let on_screen = painter.with_clip_rect(screen);
    on_screen.rect_filled(taskbar, egui::CornerRadius { sw: 3, se: 3, nw: 0, ne: 0 }, c.taskbar);
    let white = |alpha: u8| egui::Color32::from_white_alpha(alpha);
    for i in 0..5 {
        let at = egui::pos2(taskbar.center().x + (i as f32 - 2.0) * 12.0, taskbar.center().y);
        on_screen.rect_filled(egui::Rect::from_center_size(at, egui::Vec2::splat(7.0)), 2, white(if i == 0 { 235 } else { 120 }));
    }
    for i in 0..2 {
        let at = screen.left_top() + egui::vec2(24.0, 24.0 + i as f32 * 40.0);
        on_screen.rect_filled(egui::Rect::from_center_size(at, egui::Vec2::splat(20.0)), 5, white(110));
        on_screen.rect_filled(egui::Rect::from_center_size(at + egui::vec2(0.0, 17.0), egui::vec2(26.0, 4.0)), 2, white(90));
    }
    let centre = egui::pos2(screen.center().x, screen.top() + 60.0);
    app_icon(ui, &on_screen, egui::Rect::from_center_size(centre, egui::Vec2::splat(ICON)).round_to_pixels(pixels), logo, (icon, name, c.good));
}

/// The EyeCanSee icon on the desktop of the picture, with its name: faint in a dashed frame while
/// it is offered, solid with the green mark once it is there, nothing after No. It comes and goes
/// in a moment, not at once: the eye sees it being made.
fn app_icon(ui: &egui::Ui, painter: &egui::Painter, rect: egui::Rect, logo: egui::TextureId, (icon, name, good): (Icon, &str, egui::Color32)) {
    let id = egui::Id::unique("first-start-desktop-picture");
    let seen = ui.ctx().animate_bool_with_time(id.with("seen"), icon != Icon::Gone, 0.2);
    let here = ui.ctx().animate_bool_with_time(id.with("here"), icon == Icon::Here, 0.25);
    if seen <= 0.0 {
        return;
    }
    let white = |alpha: f32| egui::Color32::from_white_alpha((alpha * seen * 255.0) as u8);
    let font = egui::FontId::proportional(12.0);
    let label = painter.layout_no_wrap(name.to_owned(), font, egui::Color32::PLACEHOLDER);
    let label_at = egui::pos2(rect.center().x - label.size().x / 2.0, rect.bottom() + 8.0);
    // Around the icon and its name: the plate of a chosen icon once it is there, dashes before.
    let half = (rect.width() / 2.0 + 10.0).max(label.size().x / 2.0 + 6.0);
    let plate = egui::Rect::from_x_y_ranges(rect.center().x - half..=rect.center().x + half, rect.top() - 8.0..=label_at.y + label.size().y + 6.0);
    painter.rect(plate, 5, white(0.16 * here), egui::Stroke::new(1.0, white(0.5 * here)), egui::StrokeKind::Inside);
    if here < 1.0 {
        let p = plate.shrink(0.5);
        let around = [p.left_top(), p.right_top(), p.right_bottom(), p.left_bottom(), p.left_top()];
        painter.extend(egui::Shape::dashed_line(&around, egui::Stroke::new(1.0, white(0.8 * (1.0 - here))), 4.0, 3.0));
    }
    let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    painter.image(logo, rect, uv, white(0.55 + 0.45 * here));
    let shadow = egui::Color32::from_black_alpha((seen * (90.0 + 80.0 * here)) as u8);
    painter.galley(label_at + egui::vec2(0.0, 1.0), label.clone(), shadow);
    painter.galley(label_at, label, white(0.8 + 0.2 * here));
    if here > 0.0 {
        // The green mark of what is here, at the icon's corner away from its name, growing as
        // it comes.
        let mark = egui::Rect::from_center_size(rect.right_top() + egui::vec2(-2.0, 2.0), egui::Vec2::splat(20.0 * here));
        painter.circle_filled(mark.center(), mark.width() / 2.0 + 2.0, egui::Color32::WHITE.gamma_multiply(seen));
        painter.circle_filled(mark.center(), mark.width() / 2.0, good.gamma_multiply(seen));
        crate::addons_ui::check(painter, mark, egui::Color32::WHITE.gamma_multiply(seen));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons_ui::tests::{click, headless};

    /// What each answer shows: nothing marked and the icon only offered before an answer and after
    /// a failure; the icon there, with the name it was made with, and the choices closed after
    /// Yes; the desktop without it after No, which Yes can still change.
    #[test]
    fn what_the_page_shows() {
        let later = Line::Weak("desktop-icon-later");
        assert_eq!(look(&Desktop::Ask, "EyeCanSee"), Look { icon: Icon::Offered, name: "EyeCanSee", marked: None, open: true, line: later });
        assert_eq!(look(&Desktop::No, "EyeCanSee"), Look { icon: Icon::Gone, name: "EyeCanSee", marked: Some(false), open: true, line: later });
        let made = Desktop::Made("EyeCanSee (2)".into());
        assert_eq!(look(&made, "EyeCanSee"), Look { icon: Icon::Here, name: "EyeCanSee (2)", marked: Some(true), open: false, line: Line::Plain("desktop-icon-done") });
        assert_eq!(look(&Desktop::There, "EyeCanSee prova"), Look { icon: Icon::Here, name: "EyeCanSee prova", marked: Some(true), open: false, line: Line::Plain("desktop-icon-there") });
        let failed = Desktop::Failed("access denied".into());
        assert_eq!(look(&failed, "EyeCanSee"), Look { icon: Icon::Offered, name: "EyeCanSee", marked: None, open: true, line: Line::Failed("access denied") });
    }

    /// The whole card is the choice: a click on it answers; closed (the icon is on the desktop) a
    /// click answers nothing.
    #[test]
    fn a_click_on_a_card_answers() {
        let (ctx, area) = headless();
        let at = area.min + egui::vec2(20.0, ANSWER / 2.0);
        let mut clicked = false;
        click(&ctx, at, |ui| clicked |= answer(ui, 250.0, "Yes", false, true));
        assert!(clicked, "an open card");
        clicked = false;
        click(&ctx, at, |ui| clicked |= answer(ui, 250.0, "No", false, false));
        assert!(!clicked, "a closed card");
    }

    /// The page drawn whole, in every state and both languages, fits the body of the window: the
    /// buttons under it do not move. No (the second card) is answered by its click; Yes, off
    /// Windows, says that it failed.
    #[test]
    fn the_page_fits_and_answers() {
        let (ctx, area) = headless();
        let logo = egui::TextureId::default();
        let body = egui::vec2(512.0, crate::first_start_ui::BODY);
        let states = [Desktop::Ask, Desktop::No, Desktop::Made("EyeCanSee".into()), Desktop::There, Desktop::Failed("x".into())];
        for lang in ["en", "it"] {
            let i18n = I18n::new(lang);
            for state in &states {
                let mut used = egui::Rect::NOTHING;
                for _ in 0..2 {
                    crate::addons_ui::tests::frame(&ctx, Vec::new(), |ui| {
                        used = ui.allocate_ui(body, |ui| page_with(ui, logo, &mut state.clone(), &i18n, || unreachable!())).response.rect;
                    });
                }
                assert!(used.height() <= body.y && used.width() <= body.x, "{lang} {state:?}: {:?}", used.size());
            }
        }
        // A click down the right half reaches No, down the left half Yes: the tests' Yes fails.
        let i18n = I18n::new("en");
        let mut d = Desktop::Ask;
        let answer_at = |x: f32, d: &mut Desktop| {
            let before = d.clone();
            for y in (0..body.y as usize).step_by(20) {
                click(&ctx, area.min + egui::vec2(x, y as f32), |ui| drop(ui.allocate_ui(body, |ui| page_with(ui, logo, d, &i18n, || Err("no desktop here".into())))));
                if *d != before {
                    break;
                }
            }
        };
        answer_at(380.0, &mut d);
        assert_eq!(d, Desktop::No);
        answer_at(120.0, &mut d);
        assert_eq!(d, Desktop::Failed("no desktop here".into()));
    }
}
