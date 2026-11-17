//! The operations of the toolbar: File, Image and Tools, each a group of line icons with the
//! name and the shortcut on hover and, when off, why. On as their menu items are. When the address
//! (the field with Copy and Paste) would be left narrower than `ADDRESS` points, Image and Tools go
//! into the "More" menu; in a narrower window, until the field has `toolbar_end::FIELD_MIN`, the
//! file operations from the last, then the Metadata button, the star and Refresh follow them
//! there (`fit`): no button is left out, down to a 400 px window.

use eframe::egui;

use crate::actions::Act;
use crate::i18n::I18n;
use crate::icons::{self, Line};
use crate::selection::{CONVERTED, fits};
use crate::{App, source, toolbar_end, transform};

/// The address is never narrower than this because of the icons: Image and Tools go into the
/// "More" menu first (at 1400 points the address was 290 wide, user 2026-11-05).
const ADDRESS: f32 = 360.0;

struct Op {
    icon: Line,
    /// Fluent id of its name, the one of its menu item.
    label: &'static str,
    keys: String,
    act: Act,
    /// Why it cannot be used now.
    off: Option<String>,
}

impl App {
    fn file_ops(&self) -> Vec<Op> {
        let t = |id| self.i18n.t(id);
        let (rename, delete) = (
            Op { icon: Line::Rename, label: "menu-rename", keys: "F2".into(), act: Act::Rename(None), off: self.rename_off() },
            Op { icon: Line::Delete, label: "menu-delete", keys: t("key-delete"), act: Act::Delete(None, false), off: self.delete_off() },
        );
        // In the viewer only what works on the image shown: making folders, cutting, copying and
        // pasting files are for the grid; they stay in the menus.
        if self.viewing.is_some() {
            return vec![rename, delete];
        }
        vec![
            Op { icon: Line::NewFolder, label: "menu-new-folder", keys: t("key-new-folder"), act: Act::NewFolder(None), off: self.new_folder_off() },
            // Cut before Copy, as Explorer and the Edit menu.
            Op { icon: Line::Cut, label: "menu-cut", keys: t("key-cut"), act: Act::Cut(None), off: self.clip_off() },
            Op { icon: Line::Copy, label: "menu-copy", keys: t("key-copy"), act: Act::Copy(None), off: self.clip_off() },
            Op { icon: Line::Paste, label: "menu-paste", keys: t("key-paste"), act: Act::Paste(None), off: self.paste_off() },
            rename,
            delete,
        ]
    }

    fn image_ops(&self) -> Vec<Op> {
        let t = |id| self.i18n.t(id);
        let viewing = self.viewing.is_some();
        // As the Image menu: in the viewer the picture shown, in the grid the pictures chosen.
        let rotate = |op, icon, label, keys| Op { icon, label, keys: t(keys), act: Act::Transform(op, None), off: self.turn_op_off(op) };
        vec![
            rotate(transform::Op::Ccw, Line::RotateCcw, "tf-rotate-ccw", "key-rotate-ccw"),
            rotate(transform::Op::Cw, Line::RotateCw, "tf-rotate-cw", "key-rotate-cw"),
            Op { icon: Line::Edit, label: "menu-edit-image", keys: t("key-edit"), act: Act::Edit(None), off: self.editor_off() },
            Op { icon: Line::Save, label: "menu-save", keys: t("key-save"), act: Act::Save, off: self.save_off() },
            // Ctrl+Shift+S works in the viewer only.
            Op { icon: Line::SaveAs, label: self.save_as_label(), keys: if viewing { t("key-save-copy") } else { String::new() }, act: Act::Convert(None), off: self.save_as_off() },
        ]
    }

    /// None in the viewer: they work on folders and on the grid's choice.
    fn tool_ops(&self) -> Vec<Op> {
        if self.viewing.is_some() {
            return Vec::new();
        }
        let t = |id| self.i18n.t(id);
        vec![
            Op { icon: Line::Batch, label: "menu-batch", keys: t("key-batch"), act: Act::Batch(None), off: self.batch_off() },
            Op { icon: Line::Duplicates, label: "menu-duplicates", keys: t("key-duplicates"), act: Act::Duplicates(None), off: self.duplicates_off() },
        ]
    }

    /// Why Convert… cannot start (also for Ctrl+B): as the Tools menu.
    pub(crate) fn batch_off(&self) -> Option<String> {
        self.tools_off().or_else(|| fits(&self.grid_chosen(None), CONVERTED).is_none().then(|| self.i18n.t("tb-off-batch")))
    }

    /// Why no entry of Tools on the files chosen can start now: in the viewer, one conversion
    /// at a time (the entries wait for the one running), or in a trash.
    pub(crate) fn tools_off(&self) -> Option<String> {
        let t = |id| self.i18n.t(id);
        match () {
            _ if self.viewing.is_some() => Some(t("tb-off-viewer")),
            _ if self.batch.is_some() => Some(t("tb-off-busy")),
            // The files in a trash are left out of every work (`grid_items`).
            _ if self.chosen_in_trash() => Some(t("tb-off-trash")),
            _ => None,
        }
    }

    /// Why Find duplicates cannot start (also for Ctrl+D): as the Tools menu.
    pub(crate) fn duplicates_off(&self) -> Option<String> {
        let t = |id| self.i18n.t(id);
        match () {
            _ if self.viewing.is_some() => Some(t("tb-off-viewer")),
            _ if self.loader.is_none() => Some(t("tb-off-no-folder")),
            _ if source::is_trash(&self.dir) => Some(t("tb-off-trash")),
            _ => None,
        }
    }

    /// Refresh, the Metadata button and the star, as items of the "More" menu: those `fit` put
    /// there, before and after the groups, as on the bar.
    fn loose_ops(&self, fit: Fit) -> (Vec<Op>, Vec<Op>) {
        let mut before = Vec::new();
        if self.viewing.is_none() && !fit.refresh() {
            before.push(Op { icon: Line::Refresh, label: "menu-refresh", keys: "F5".into(), act: Act::Refresh, off: None });
        }
        let mut after = Vec::new();
        if self.viewing.is_none() && !fit.meta() {
            after.push(Op { icon: Line::Info, label: "tb-meta", keys: String::new(), act: Act::MetaAll, off: None });
        }
        if !fit.star() {
            let have = self.here_kept();
            let (icon, act) = if have { (Line::StarFilled, Act::RemoveBookmark(None)) } else { (Line::Star, Act::AddBookmark(None)) };
            let off = self.loader.is_none().then(|| self.i18n.t("tb-off-no-folder"));
            after.push(Op { icon, label: self.star_label(), keys: String::new(), act, off });
        }
        (before, after)
    }

    /// What of the toolbar's first row stays on it, from the width it has: called at its start.
    pub(crate) fn toolbar_fit(&self, ui: &egui::Ui) -> Fit {
        let grid = self.viewing.is_none();
        let row = Row { grid, file: self.file_ops().len(), image: self.image_ops().len(), tools: self.tool_ops().len() };
        let sort = if grid { toolbar_end::sort_width(toolbar_end::TailParts::measure(ui)) } else { 0.0 };
        fit(row, RowParts::measure(ui), ui.available_width(), sort)
    }

    /// The three groups, after Up (or Grid) and Refresh, with what `fit` leaves on the bar; the
    /// rest in the "More" menu. The one chosen runs after the frame.
    pub(crate) fn toolbar_ops(&mut self, ui: &mut egui::Ui, fit: Fit) {
        let mut chosen = None;
        let file = self.file_ops();
        let (kept, gone) = file.split_at(fit.file_kept());
        if !kept.is_empty() {
            group(ui, kept, &self.i18n, &mut chosen);
        }
        let (image, tools) = (self.image_ops(), self.tool_ops());
        if fit.groups {
            group(ui, &image, &self.i18n, &mut chosen);
            if !tools.is_empty() {
                group(ui, &tools, &self.i18n, &mut chosen);
            }
        } else {
            let (before, after) = self.loose_ops(fit);
            ui.separator();
            more(ui, &[&before, gone, &image, &tools, &after], &self.i18n, &mut chosen);
        }
        ui.separator();
        if chosen.is_some() {
            self.act = chosen;
        }
    }
}

/// Width of a separator in a row, without the room before it: egui's default.
const SEPARATOR: f32 = 6.0;
/// Width of the small arrow in the Bookmarks button.
const BOOKMARK_DOWN: f32 = 14.0;

/// The widths the toolbar's first row is made of, in points, each with the room before it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowParts {
    spacing: f32,
    /// A square button with a line icon.
    button: f32,
    separator: f32,
    /// The Bookmarks drop-down: its icon and the arrow.
    bookmarks: f32,
}

impl RowParts {
    /// From the style: button padding, item spacing, room between the icons of a button.
    pub(crate) fn new(padding: f32, spacing: f32, icon_spacing: f32) -> Self {
        let bookmarks = icons::BUTTON.x.max(icons::ICON + icon_spacing + BOOKMARK_DOWN + 2.0 * padding);
        Self { spacing, button: spacing + icons::BUTTON.x.max(icons::ICON + 2.0 * padding), separator: spacing + SEPARATOR, bookmarks: spacing + bookmarks }
    }

    pub(crate) fn measure(ui: &egui::Ui) -> Self {
        let s = ui.spacing();
        Self::new(s.button_padding.x, s.item_spacing.x, s.icon_spacing)
    }
}

/// The buttons of the toolbar's first row, before the address field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Row {
    /// In the grid: with Refresh and the Metadata button.
    pub(crate) grid: bool,
    /// File operations: 6 in the grid, Rename and Delete in the viewer.
    pub(crate) file: usize,
    pub(crate) image: usize,
    /// None in the viewer.
    pub(crate) tools: usize,
}

impl Row {
    /// The buttons that can go into "More" one by one: the file operations, the star, and in the
    /// grid Refresh and the Metadata button.
    fn loose(self) -> usize {
        self.file + 1 + 2 * usize::from(self.grid)
    }
}

/// What stays on the toolbar's first row: Image and Tools (`groups`) or the "More" menu with
/// them; and how many of the `Row::loose` buttons went into it, in this order: the file
/// operations from the last, the Metadata button, the star, Refresh. Up (or Grid), the Bookmarks
/// drop-down and the address field always stay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fit {
    pub(crate) groups: bool,
    gone: usize,
    row: Row,
}

impl Fit {
    /// The `rank`-th button to go is still on the bar.
    fn kept(self, rank: usize) -> bool {
        self.gone <= rank
    }

    /// File operations on the bar, from the first.
    pub(crate) fn file_kept(self) -> usize {
        self.row.file - self.gone.min(self.row.file)
    }

    pub(crate) fn meta(self) -> bool {
        self.row.grid && self.kept(self.row.file)
    }

    pub(crate) fn star(self) -> bool {
        self.kept(self.row.file + usize::from(self.row.grid))
    }

    pub(crate) fn refresh(self) -> bool {
        self.row.grid && self.kept(self.row.file + 2)
    }

    /// The row's width before the address field, as `App::toolbar` draws it, with the room
    /// before the field.
    fn width(self, p: RowParts) -> f32 {
        let buttons = |n: usize| n as f32 * p.button;
        let bar = |on: bool| if on { p.button } else { 0.0 };
        // Up or Grid, the one never gone: the room before it is counted too, to be safe.
        let mut w = p.button + bar(self.refresh());
        let file = self.file_kept();
        if file > 0 {
            w += p.separator + buttons(file);
        }
        if self.groups {
            w += p.separator + buttons(self.row.image);
            if self.row.tools > 0 {
                w += p.separator + buttons(self.row.tools);
            }
        } else {
            w += p.separator + p.button;
        }
        w + p.separator + bar(self.meta()) + bar(self.star()) + p.bookmarks + p.spacing
    }
}

/// What of `row` stays on the bar in `available` points: all of it when the address (the field
/// with Copy and Paste) keeps `ADDRESS` points after it and `sort` (the grid's sorting and
/// slider); otherwise Image and Tools go into "More", and then the loose buttons one by one, until
/// the field alone keeps `FIELD_MIN` (what follows it goes on the row under it then).
pub(crate) fn fit(row: Row, p: RowParts, available: f32, sort: f32) -> Fit {
    let all = Fit { groups: true, gone: 0, row };
    if available - all.width(p) - sort >= ADDRESS {
        return all;
    }
    (0..=row.loose())
        .map(|gone| Fit { groups: false, gone, row })
        .find(|f| available - f.width(p) >= toolbar_end::FIELD_MIN)
        .unwrap_or(Fit { groups: false, gone: row.loose(), row })
}

/// "Name (keys)".
fn tip(i18n: &I18n, op: &Op) -> String {
    let name = i18n.t(op.label);
    if op.keys.is_empty() { name } else { format!("{name} ({})", op.keys) }
}

fn group(ui: &mut egui::Ui, ops: &[Op], i18n: &I18n, chosen: &mut Option<Act>) {
    ui.separator();
    for op in ops {
        let tip = tip(i18n, op);
        let r = ui.add_enabled(op.off.is_none(), icons::button(ui, op.icon)).on_hover_text(&tip);
        let r = match &op.off {
            Some(why) => r.on_disabled_hover_text(format!("{tip}\n{why}")),
            None => crate::browser::with_about(ui, r, &op.act),
        };
        if r.clicked() {
            *chosen = Some(op.act.clone());
        }
    }
}

/// What does not fit on the bar in a menu, as menu items: icon, name, shortcut; a separator
/// between groups, the empty ones left out.
fn more(ui: &mut egui::Ui, groups: &[&[Op]], i18n: &I18n, chosen: &mut Option<Act>) {
    let button = icons::button(ui, Line::More);
    let (r, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        for (n, ops) in groups.iter().filter(|g| !g.is_empty()).enumerate() {
            if n > 0 {
                ui.separator();
            }
            for op in *ops {
                let item = egui::Button::image_and_text(icons::line(ui, op.icon, 16.0), i18n.t(op.label)).shortcut_text(op.keys.clone());
                let r = ui.add_enabled(op.off.is_none(), item);
                let r = match &op.off {
                    Some(why) => r.on_disabled_hover_text(why),
                    None => r,
                };
                if r.clicked() {
                    *chosen = Some(op.act.clone());
                }
            }
        }
    });
    r.on_hover_text(i18n.t("tb-more"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolbar_end::FIELD_MIN;

    /// egui's default style: padding 8, spacing 8, 4 between the icons of a button.
    fn parts() -> RowParts {
        RowParts::new(8.0, 8.0, 4.0)
    }

    const GRID: Row = Row { grid: true, file: 6, image: 5, tools: 2 };
    const VIEWER: Row = Row { grid: false, file: 2, image: 5, tools: 0 };

    /// The sum, part by part: buttons 34 wide (18 + 8 + 8), separators 6, the Bookmarks drop-down
    /// 52 (18 + 4 + 14 + 8 + 8), 8 before each item and before the field.
    #[test]
    fn width_part_by_part() {
        let p = parts();
        let all = Fit { groups: true, gone: 0, row: GRID };
        // Up, Refresh | 6 | 5 | 2 | Metadata, star, Bookmarks.
        assert_eq!(all.width(p), 2.0 * 42.0 + 14.0 + 6.0 * 42.0 + 14.0 + 5.0 * 42.0 + 14.0 + 2.0 * 42.0 + 14.0 + 2.0 * 42.0 + 60.0 + 8.0);
        // All gone: Up | More | Bookmarks.
        let least = Fit { groups: false, gone: GRID.loose(), row: GRID };
        assert_eq!(least.width(p), 42.0 + 14.0 + 42.0 + 14.0 + 60.0 + 8.0);
        let least = Fit { groups: false, gone: VIEWER.loose(), row: VIEWER };
        assert_eq!(least.width(p), 42.0 + 14.0 + 42.0 + 14.0 + 60.0 + 8.0);
    }

    /// The order things go into "More": Image and Tools together, then the file operations from
    /// the last, the Metadata button, the star, Refresh; in the viewer Delete, Rename, the star.
    #[test]
    fn order() {
        let f = |row, gone| Fit { groups: false, gone, row };
        assert_eq!(f(GRID, 1).file_kept(), 5);
        assert!(f(GRID, 6).meta() && f(GRID, 6).star() && f(GRID, 6).refresh());
        assert_eq!(f(GRID, 6).file_kept(), 0);
        assert!(!f(GRID, 7).meta() && f(GRID, 7).star());
        assert!(!f(GRID, 8).star() && f(GRID, 8).refresh());
        assert!(!f(GRID, 9).refresh());
        assert!(f(VIEWER, 2).star() && !f(VIEWER, 3).star());
        assert!(!f(VIEWER, 0).meta() && !f(VIEWER, 0).refresh());
        // Narrower and narrower: never back on the bar once gone.
        for row in [GRID, VIEWER] {
            let mut last = fit(row, parts(), 2000.0, 300.0);
            assert!(last.groups && last.gone == 0);
            for available in (200..2000).rev() {
                let now = fit(row, parts(), available as f32, 300.0);
                assert!(now.gone >= last.gone && (last.groups || !now.groups), "{row:?} at {available}");
                last = now;
            }
            assert_eq!(last.gone, row.loose());
        }
    }

    /// From a 400 px window up, the field keeps its least width; below the least row, it gets
    /// what is left.
    #[test]
    fn field_kept() {
        for row in [GRID, VIEWER] {
            for available in 380..2000 {
                let f = fit(row, parts(), available as f32, 300.0);
                assert!(available as f32 - f.width(parts()) >= FIELD_MIN, "{row:?} at {available}: {f:?}");
            }
        }
        // The first width where something must go: one point less than the whole row and its address.
        let all = Fit { groups: true, gone: 0, row: GRID };
        assert!(fit(GRID, parts(), all.width(parts()) + 300.0 + ADDRESS, 300.0).groups);
        assert!(!fit(GRID, parts(), all.width(parts()) + 300.0 + ADDRESS - 1.0, 300.0).groups);
    }

    /// Through egui with the app's fonts and the real widgets, at 100 %, 110 % and 125 %, in
    /// windows from 400 to 1400 px, in the grid and in the viewer: the row as `fit` keeps it
    /// leaves the field at least `FIELD_MIN` wide, inside the window. The least row with its field
    /// takes about 356 points (Up, More, Bookmarks, the field, the panel's margins): 400 px at
    /// 110 %, 450 px at 125 %; narrower windows are left out.
    #[test]
    fn real_window() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let mut checked = 0;
        for ppp in [1.0, 1.1, 1.25] {
            ctx.set_pixels_per_point(ppp);
            for px in [400.0, 450.0, 520.0, 600.0, 700.0, 800.0, 1000.0, 1200.0, 1400.0] {
                if px / ppp < 360.0 {
                    continue;
                }
                for row in [GRID, VIEWER] {
                    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(px / ppp, 600.0))), ..Default::default() };
                    let mut seen = (egui::Rect::NOTHING, 0.0);
                    for _ in 0..3 {
                        ctx.run_ui(input.clone(), |ui| {
                            egui::Panel::top("toolbar").show(ui, |ui| seen = draw(ui, row));
                        })
                        .textures_delta
                        .clear();
                    }
                    let (field, right) = seen;
                    let what = format!("{px} px at {ppp}, {row:?}");
                    assert!(field.width() >= FIELD_MIN - 0.01, "{what}: field {field:?}");
                    assert!(field.right() <= right + 0.01, "{what}: field {field:?} past {right}");
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 52);
    }

    /// The first row with stand-ins as wide as the real buttons, as `fit` keeps it, then the field
    /// to the end: the field and the right edge.
    fn draw(ui: &mut egui::Ui, row: Row) -> (egui::Rect, f32) {
        let right = ui.max_rect().right();
        let mut field = egui::Rect::NOTHING;
        ui.horizontal(|ui| {
            let f = fit(row, RowParts::measure(ui), ui.available_width(), crate::toolbar_end::sort_width(crate::toolbar_end::TailParts::measure(ui)));
            let buttons = |ui: &mut egui::Ui, n: usize| (0..n).for_each(|_| _ = ui.add(icons::button(ui, Line::Up)));
            buttons(ui, 1 + usize::from(f.refresh()));
            if f.file_kept() > 0 {
                ui.separator();
                buttons(ui, f.file_kept());
            }
            if f.groups {
                ui.separator();
                buttons(ui, row.image);
                if row.tools > 0 {
                    ui.separator();
                    buttons(ui, row.tools);
                }
            } else {
                ui.separator();
                buttons(ui, 1);
            }
            ui.separator();
            for on in [f.meta(), f.star()] {
                if on {
                    ui.add(icons::toggle(ui, true, Line::Info));
                }
            }
            ui.add(crate::bookmarks::menu_button(ui));
            let available = ui.available_width();
            field = ui.add(egui::TextEdit::singleline(&mut String::from("/home/demo/Pictures")).desired_width(available)).rect;
        });
        (field, right)
    }
}
