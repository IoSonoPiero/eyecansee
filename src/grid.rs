//! Thumbnail grid: cells, the ".." cell and the trash strip, selection by mouse, keys and
//! rectangle, the scroll bar dragged outside the window, and the order thumbnails are made in.

use eframe::egui::{self, Align2, Color32, FontId, Key, Label, Rect, ScrollArea, Sense, Ui, pos2, vec2};

use std::collections::HashSet;

use crate::actions::Act;
use crate::browser::{context_menu, error_text, focus_frame, selection_fill};
use crate::icons::{self, Icon};
use crate::{App, Focus};
use crate::source;

/// Thumbnail rows loaded right after the visible ones, above and below.
const MARGIN_ROWS: usize = 3;

/// Thumbnails kept in memory, in a window around the visible ones: beyond it, when scrolling, the
/// distant ones are reloaded from disk and disappear for a moment. At most 256×256 RGBA, so
/// about 250 MB in the worst case; a smaller folder stays entirely in memory.
const MAX_THUMBS: usize = 1000;

/// Length of the scroll bar handle, as egui computes it: proportional to the visible part,
/// never below the style's minimum.
fn handle_len(ui: &Ui, rect: Rect, content: f32) -> f32 {
    let h = rect.height();
    (h * h / content.max(h)).max(ui.spacing().scroll.handle_min_length).min(h)
}

/// Top edge of the handle for a given scroll offset.
fn handle_top(ui: &Ui, rect: Rect, content: f32, offset: f32) -> f32 {
    let max = (content - rect.height()).max(1.0);
    rect.top() + offset / max * (rect.height() - handle_len(ui, rect, content))
}

impl App {
    #[allow(clippy::too_many_lines)] // to be split
    pub(crate) fn grid(&mut self, ui: &mut Ui) {
        if ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(ui.max_rect()) {
            self.focus = Focus::Grid;
        }
        // Right click on the empty space: the actions of the whole folder. The cells come later
        // and are on top, so they keep their own menu.
        // Dragged, it draws a rectangle that selects the cells it touches.
        let background = ui.interact(ui.max_rect(), ui.scope_id().with("grid-background"), Sense::click_and_drag());
        let band = (background.drag_started(), background.dragged(), ui.input(|i| i.modifiers.command));
        if background.drag_stopped() {
            self.band = None;
        }
        background.context_menu(|ui| {
            let t = |id| self.i18n.t(id);
            let mut items = Vec::new();
            // The main action here, when there is something to paste.
            if self.can_paste(&self.dir) {
                items.push(Some((t("menu-paste"), t("key-paste"), Act::Paste(None))));
                items.push(None);
            }
            items.push(Some((t("menu-refresh"), "F5".into(), Act::Refresh)));
            if !self.dir.is_empty() {
                items.push(Some((t("menu-parent"), t("key-backspace"), Act::Up)));
            }
            // Inside the trash: what its own right-click menu offered when it was a cell.
            if source::is_trash(&self.dir) {
                let dir = Some(self.dir.clone());
                items.push(None);
                items.push(Some((t("menu-restore-all"), String::new(), Act::Restore(dir.clone()))));
                items.push(Some((t("menu-empty-trash"), String::new(), Act::Delete(dir, true))));
            }
            if let Some(a) = context_menu(ui, items) {
                self.act = Some(a);
            }
        });
        if self.focus == Focus::Grid {
            focus_frame(ui);
        }
        self.trash_bar(ui);
        let Some(Ok(listing)) = self.listings.get(&self.dir) else {
            ui.centered_and_justified(|ui| match self.listings.get(&self.dir) {
                Some(Err(e)) => ui.label(error_text(ui, e.as_str()).size(16.0)),
                _ => ui.spinner(),
            });
            return;
        };
        let (ndirs, nfiles) = (listing.dirs.len(), listing.files.len());
        let n = ndirs + nfiles;
        // First cell "..", to go up with the mouse too; not at the root, where there is no up.
        let up = usize::from(!self.dir.is_empty());
        if n == 0 && up == 0 {
            let text = match self.phone_root(&self.dir) {
                Some(d) => error_text(ui, self.i18n.t_args("error-no-storage", &[("detail", &d.name)])).size(16.0),
                None => egui::RichText::new(self.i18n.t("folder-empty")),
            };
            // Not selectable: a selectable label filling the grid took the right click from the
            // background, so an empty drive offered no Paste.
            ui.centered_and_justified(|ui| ui.add(egui::Label::new(text).selectable(false)));
            return;
        }
        let total = n + up;

        // Bar always visible and wide: the thin one that widens on hover was hard to grab. If
        // egui loses the drag (see `BarDrag`), `bar_offset` continues it.
        // Handle grey like the weak text, darker under the mouse: with the button colour it was
        // barely distinguishable from the light background.
        let mut scroll = egui::style::ScrollStyle::solid();
        scroll.bar_width = 10.0;
        let side = self.thumb_size;
        let cell = vec2(side + 16.0, side + 32.0);
        let cols = (((ui.available_width() - scroll.allocated_width()) / cell.x).floor() as usize).max(1);
        let rows = total.div_ceil(cols);

        let mut visible_rows = 0..0;
        // Double click and keys (Enter, Backspace) run once the grid is finished: changing folder
        // before or during drawing, the cells would read the new listing with the old counts
        // (crash of 2026-10-20 when re-entering a folder already visited).
        let mut open = None;
        let mut go_up = false;
        ui.spacing_mut().scroll = scroll;
        let (weak, strong, selection) = (ui.visuals().weak_text_color(), ui.visuals().text_color(), ui.visuals().selection.bg_fill);
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.bg_fill = weak;
        widgets.hovered.bg_fill = strong;
        widgets.active.bg_fill = selection;
        let content = rows as f32 * cell.y;
        // One position per folder, so with `remember_scroll` each one is found where it was;
        // without it, every folder opened starts from the top.
        let mut area = ScrollArea::vertical().id_salt(("grid", &self.dir)).auto_shrink(false);
        if std::mem::take(&mut self.scroll_top) && !self.settings.remember_scroll {
            area = area.vertical_scroll_offset(0.0);
        }
        if let Some(rect) = self.grid_rect
            && let Some(offset) = self.bar_offset(ui, rect, content)
        {
            area = area.vertical_scroll_offset(offset);
        }
        let out = area.show_viewport(ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            ui.set_height(rows as f32 * cell.y);
            // The width left over is shared among the columns: it used to stay as a gap on the right.
            let width = (ui.max_rect().width() / cols as f32).max(cell.x);
            // `p`: position in the grid, the ".." cell included.
            let cell_rect = |p: usize| {
                let pos = origin + vec2((p % cols) as f32 * width, (p / cols) as f32 * cell.y);
                Rect::from_min_size(pos, vec2(width, cell.y))
            };
            let first = (viewport.min.y / cell.y).floor().max(0.0) as usize;
            let last = ((viewport.max.y / cell.y).ceil() as usize).min(rows);
            visible_rows = first..last;
            for p in first * cols..(last * cols).min(total) {
                if p < up {
                    go_up |= self.up_cell(ui, cell_rect(p));
                } else if self.grid_cell(ui, cell_rect(p), p - up, ndirs) {
                    open = Some(p - up);
                }
            }
            let pointer = ui.ctx().pointer_interact_pos();
            if let (true, _, keep, Some(at)) = (band.0, band.1, band.2, pointer) {
                let before = if keep { self.marked.clone() } else { HashSet::new() };
                self.band = Some(((at - origin).to_pos2(), before));
            }
            if let (Some((start, before)), true, Some(at)) = (&self.band, band.1, pointer) {
                let start = origin + start.to_vec2();
                let r = Rect::from_two_pos(start, at);
                let mut marked = before.clone();
                marked.extend((up..total).filter(|&p| cell_rect(p).shrink(6.0).intersects(r)).filter_map(|p| self.item_path(p - up)));
                self.marked = marked;
                let accent = crate::theme::accent(ui.visuals());
                ui.painter().rect(r, 2.0, accent.gamma_multiply(0.15), egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
                // Near the top or bottom edge the grid scrolls, to reach the cells out of sight.
                let edge = 24.0;
                let clip = ui.clip_rect();
                let dy = if at.y < clip.min.y + edge { 12.0 } else if at.y > clip.max.y - edge { -12.0 } else { 0.0 };
                if dy != 0.0 {
                    ui.scroll_with_delta(vec2(0.0, dy));
                }
                ui.ctx().request_repaint();
            }
            if std::mem::take(&mut self.scroll_to_selected) {
                let p = if self.cursor { self.selected + up } else { 0 };
                ui.scroll_to_rect(cell_rect(p), None);
            }
        });
        if n == 0 {
            let font = egui::TextStyle::Body.resolve(ui.style());
            ui.painter().text(out.inner_rect.center(), Align2::CENTER_CENTER, self.i18n.t("folder-empty"), font, ui.visuals().text_color());
        }
        self.grid_rect = Some(out.inner_rect);
        self.track_bar(ui, out.inner_rect, content, out.state.clamped_offset().y);
        self.prefetch_thumbs(visible_rows, cols, up, ndirs, nfiles);
        if go_up {
            self.go_up();
        } else if let Some(i) = open {
            self.activate(i);
        } else if self.focus == Focus::Grid {
            self.grid_keys(ui.ctx(), cols, n, up);
        }
    }

    /// Follows the drag of the grid's scroll bar: it starts with a press on the bar's strip (to
    /// the right of `rect`, the cell area) and ends on release. While egui follows it, remembers
    /// where on the handle the mouse is (`grab`).
    pub(crate) fn track_bar(&mut self, ui: &Ui, rect: Rect, content: f32, offset: f32) {
        let (pressed, down, pos) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.latest_pos()));
        let strip = Rect::from_x_y_ranges(rect.right()..=rect.right() + ui.spacing().scroll.allocated_width(), rect.y_range());
        let top = handle_top(ui, rect, content, offset);
        if pressed && let Some(p) = pos && strip.contains(p) {
            self.bar_drag = Some(crate::BarDrag { grab: p.y - top, lost: false });
        }
        match (&mut self.bar_drag, pos) {
            (Some(_), _) if !down => self.bar_drag = None,
            (Some(d), None) => d.lost = true,
            (Some(d), Some(p)) if !d.lost => d.grab = p.y - top,
            _ => {}
        }
    }

    /// Mouse back in with the button still pressed: egui has let go of the bar (egui 0.37's
    /// `interaction.rs` ends the drag when the pointer leaves the window, and on Wayland passing
    /// over GNOME's top bar is enough). From here EyeCanSee decides the scrolling: the handle
    /// returns under the mouse, at the point where it was grabbed.
    pub(crate) fn bar_offset(&self, ui: &Ui, rect: Rect, content: f32) -> Option<f32> {
        let d = self.bar_drag.as_ref().filter(|d| d.lost)?;
        let p = ui.input(|i| i.pointer.latest_pos())?;
        let max = (content - rect.height()).max(0.0);
        let travel = rect.height() - handle_len(ui, rect, content);
        if max <= 0.0 || travel <= 0.0 {
            return None;
        }
        Some(((p.y - d.grab - rect.top()) / travel * max).clamp(0.0, max))
    }

    /// Strip at the top of the grid, only where there is a trash: in a folder that has one, the
    /// button to open it; inside a trash, whose it is and the actions on all of it. The buttons
    /// run at the end of the frame (`App::act`).
    pub(crate) fn trash_bar(&mut self, ui: &mut Ui) {
        let inside = source::is_trash(&self.dir);
        let trash = match self.listings.get(&self.dir) {
            Some(Ok(l)) => l.trash.clone(),
            _ => None,
        };
        if trash.is_none() && !inside {
            return;
        }
        let t = |id| self.i18n.t(id);
        let mut act = None;
        let [fill, stroke, text, button] = crate::theme::strip(ui.visuals());
        let frame = egui::Frame::group(ui.style()).fill(fill).stroke(egui::Stroke::new(1.0, stroke)).inner_margin(egui::Margin::symmetric(10, 6));
        frame.show(ui, |ui| {
            // Buttons with their border always shown: without it they looked like plain text.
            ui.visuals_mut().override_text_color = Some(text);
            ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, button);
            ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::new(1.5, button);
            ui.horizontal(|ui| {
                ui.add(icons::image(ui.ctx(), Icon::Trash, 20.0));
                if inside {
                    let folder = source::parent(&self.dir);
                    let name = if folder.is_empty() { self.base.clone() } else { source::name(&folder).to_string() };
                    ui.label(self.i18n.t_args("trash-bar-inside", &[("folder", &name)]));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dir = Some(self.dir.clone());
                        if ui.button(t("menu-empty-trash")).clicked() {
                            act = Some(Act::Delete(dir.clone(), true));
                        }
                        if ui.button(t("menu-restore-all")).clicked() {
                            act = Some(Act::Restore(dir));
                        }
                    });
                } else if let Some(trash) = trash {
                    ui.label(t("trash-bar-has"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("trash-open")).clicked() {
                            act = Some(Act::OpenDir(trash));
                        }
                    });
                }
            });
        });
        if act.is_some() {
            self.act = act;
        }
    }

    /// Draws the ".." cell, which goes up one level; returns `true` if it was double-clicked.
    pub(crate) fn up_cell(&mut self, ui: &mut Ui, rect: Rect) -> bool {
        let name = self.i18n.t("menu-parent");
        let mut response = ui.interact(rect, ui.scope_id().with("cell-up"), Sense::click());
        if !self.asking() {
            response = response.on_hover_text(&name);
        }
        let painter = ui.painter_at(rect);
        let fill = selection_fill(ui, self.focus == Focus::Grid);
        let visuals = ui.visuals();
        if self.up_selected && !self.cursor {
            painter.rect_filled(rect.shrink(2.0), 6.0, fill);
        } else if response.hovered() {
            painter.rect_filled(rect.shrink(2.0), 6.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let side = self.thumb_size;
        let thumb_rect = Rect::from_min_size(pos2(rect.center().x - side / 2.0, rect.min.y + 6.0), vec2(side, side));
        icons::paint(&painter, Icon::Up, Rect::from_center_size(thumb_rect.center(), vec2(side, side) * 0.45));
        let label_rect = Rect::from_min_size(pos2(rect.min.x + 4.0, thumb_rect.max.y + 7.0), vec2(rect.width() - 8.0, 18.0));
        ui.put(label_rect, Label::new(egui::RichText::new("..").color(visuals.text_color())).selectable(false));
        if response.clicked() || response.secondary_clicked() {
            self.up_selected = true;
            self.cursor = false;
            self.focus = Focus::Grid;
        }
        response.context_menu(|ui| {
            let items = vec![Some((self.i18n.t("menu-parent"), self.i18n.t("key-backspace"), Act::Up))];
            if let Some(a) = context_menu(ui, items) {
                self.act = Some(a);
            }
        });
        response.double_clicked()
    }

    /// Draws a cell; returns `true` if it was double-clicked.
    #[allow(clippy::too_many_lines)] // to be split
    pub(crate) fn grid_cell(&mut self, ui: &mut Ui, rect: Rect, i: usize, ndirs: usize) -> bool {
        let Some(Ok(listing)) = self.listings.get(&self.dir) else { return false };
        let path = if i < ndirs { listing.dirs[i].clone() } else { listing.files[i - ndirs].path.clone() };
        let name = self.item_name(&path);
        let mut response = ui.interact(rect, ui.scope_id().with(("cell", i)), Sense::click());
        // Not under a confirmation: the name popping up next to it looked like a highlighted item.
        if !self.asking() {
            response = response.on_hover_text(&name);
        }

        let painter = ui.painter_at(rect);
        let active = self.focus == Focus::Grid;
        let fill = selection_fill(ui, active);
        let visuals = ui.visuals();
        // Marked items, if any, are the selection; otherwise the cursor's item.
        let selected = match self.marked.is_empty() {
            true => self.cursor && i == self.selected,
            false => self.marked.contains(&path),
        };
        // A selected folder fills its cell; a selected image colours its tile instead.
        if selected && i < ndirs {
            painter.rect_filled(rect.shrink(2.0), 6.0, fill);
        } else if response.hovered() {
            painter.rect_filled(rect.shrink(2.0), 6.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let side = self.thumb_size;
        // Centred: cells widen to fill the row.
        let thumb_rect = Rect::from_min_size(pos2(rect.center().x - side / 2.0, rect.min.y + 6.0), vec2(side, side));
        let dim = visuals.weak_text_color();
        let accent = crate::theme::accent(visuals);
        if i < ndirs {
            icons::paint(&painter, Icon::Folder, Rect::from_center_size(thumb_rect.center(), vec2(side, side) * 0.7));
        } else {
            // Every image in its own tile, so you can see where one photo ends and the next begins
            // (not folders: they stay recognisable at a glance). Selected: thick border, tinted tile.
            let (tile, tile_stroke) = crate::theme::tile(visuals);
            let tinted = crate::theme::tile_selected(visuals);
            let (tile, stroke) = match (selected, active) {
                (true, true) => (tinted, egui::Stroke::new(3.0, accent)),
                (true, false) => (tinted, egui::Stroke::new(2.0, accent.gamma_multiply(0.6))),
                _ => (tile, egui::Stroke::new(1.0, tile_stroke)),
            };
            painter.rect(thumb_rect.expand(4.0), 6.0, tile, stroke, egui::StrokeKind::Inside);
            match self.thumbs.get(&(i - ndirs)) {
                Some(Ok(tex)) => {
                    let size = tex.size_vec2();
                    let scale = (side / size.x).min(side / size.y).min(1.0);
                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                    // In the centre of the tile: the tile keeps the photo together with its name.
                    let image = Rect::from_center_size(thumb_rect.center(), size * scale);
                    painter.image(tex.id(), image, uv, Color32::WHITE);
                }
                Some(Err(_)) => {
                    painter.text(thumb_rect.center(), Align2::CENTER_CENTER, "⚠", FontId::proportional(side * 0.3), dim);
                }
                None => {
                    painter.text(thumb_rect.center(), Align2::CENTER_CENTER, "…", FontId::proportional(24.0), dim);
                }
            }
        }
        // Copied or cut, waiting for Ctrl+V: a badge in the corner; cut items also fade.
        if let Some(clip) = &self.clip
            && clip.base == self.base
            && clip.paths.contains(&path)
        {
            if clip.cut {
                painter.rect_filled(thumb_rect.expand(4.0), 6.0, crate::theme::canvas(visuals).gamma_multiply(0.55));
            }
            let badge = Rect::from_center_size(thumb_rect.right_top() + vec2(-8.0, 8.0), vec2(22.0, 22.0));
            icons::paint(&painter, if clip.cut { Icon::Cut } else { Icon::Copied }, badge);
        }
        let label_rect = Rect::from_min_size(pos2(rect.min.x + 4.0, thumb_rect.max.y + 7.0), vec2(rect.width() - 8.0, 18.0));
        // Image names brighter than folder names (they used to be all grey on grey).
        let color = if i < ndirs { visuals.text_color() } else { visuals.text_color().lerp_to_gamma(visuals.strong_text_color(), 0.6) };
        let mut text = egui::RichText::new(name).color(color);
        if selected && i >= ndirs {
            text = text.color(accent).strong();
        }
        ui.put(label_rect, Label::new(text).truncate().selectable(false));

        if response.clicked() {
            let mods = ui.input(|i| i.modifiers);
            if mods.shift {
                self.mark_range(i);
            } else if mods.command {
                // The item selected before is marked too, so Ctrl+click adds to it.
                if self.marked.is_empty()
                    && self.cursor
                    && let Some(p) = self.item_path(self.selected)
                {
                    self.marked.insert(p);
                }
                if !self.marked.remove(&path) {
                    self.marked.insert(path.clone());
                }
                self.anchor = i;
            } else {
                self.marked.clear();
                self.anchor = i;
            }
            self.selected = i;
            self.up_selected = false;
            self.cursor = true;
        }
        // The right click selects the item too, so the menu acts on it (on all the marked ones, if
        // it is one of them).
        if response.secondary_clicked() {
            if !self.marked.contains(&path) {
                self.marked.clear();
                self.anchor = i;
            }
            self.selected = i;
            self.up_selected = false;
            self.cursor = true;
            self.focus = Focus::Grid;
        }
        response.context_menu(|ui| {
            let items = self.item_menu(Some(path.clone()), Some(Act::Open));
            if let Some(a) = context_menu(ui, items) {
                self.act = Some(a);
            }
        });
        response.double_clicked()
    }

    /// Marks the items from `anchor` to `i` (Shift+click, Shift+arrows), and only those.
    pub(crate) fn mark_range(&mut self, i: usize) {
        let (a, b) = (self.anchor.min(i), self.anchor.max(i));
        self.marked = (a..=b).filter_map(|k| self.item_path(k)).collect();
    }

    /// Arrows, Home, End, Enter, Backspace, Ctrl+A. They work on positions in the grid, the ".."
    /// cell (`up` = 1) included; Home goes to the first item, the ".." cell is reached with the
    /// arrows. With Shift they mark from the anchor, without it they clear the marks.
    pub(crate) fn grid_keys(&mut self, ctx: &egui::Context, cols: usize, n: usize, up: usize) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let total = n + up;
        let pos = match (self.cursor, self.up_selected) {
            (true, _) if n > 0 => Some(self.selected.min(n - 1) + up),
            (false, true) if up == 1 => Some(0),
            _ => None,
        };
        // With no selection shown, every movement starts from the first item.
        let first = up.min(total - 1);
        let mut to = pos;
        let (mut enter, mut back, mut shift, mut all) = (false, false, false, false);
        ctx.input(|i| {
            shift = i.modifiers.shift;
            all = i.modifiers.command && i.key_pressed(Key::A);
            for (key, step) in [(Key::ArrowRight, 1), (Key::ArrowLeft, -1), (Key::ArrowDown, cols as isize), (Key::ArrowUp, -(cols as isize))] {
                if i.key_pressed(key) {
                    to = Some(to.map_or(first, |p| p.saturating_add_signed(step).min(total - 1)));
                }
            }
            if i.key_pressed(Key::Home) {
                to = Some(first);
            }
            if i.key_pressed(Key::End) {
                to = Some(total - 1);
            }
            enter = i.key_pressed(Key::Enter) && pos.is_some();
            back = i.key_pressed(Key::Backspace);
        });
        if to != pos
            && let Some(p) = to
        {
            self.up_selected = p < up;
            self.cursor = p >= up;
            self.selected = p.saturating_sub(up);
            self.scroll_to_selected = true;
            match shift && self.cursor {
                true => self.mark_range(self.selected),
                false => {
                    self.marked.clear();
                    self.anchor = self.selected;
                }
            }
        }
        if all {
            self.marked = (0..n).filter_map(|k| self.item_path(k)).collect();
        }
        if enter && pos == Some(0) && up == 1 {
            self.go_up();
        } else if enter {
            self.activate(self.selected);
        } else if back {
            self.go_up();
        }
    }

    /// Double click or Enter: enters the folder or opens the image in the viewer.
    pub(crate) fn activate(&mut self, i: usize) {
        let Some(Ok(listing)) = self.listings.get(&self.dir) else { return };
        match listing.dirs.get(i) {
            Some(dir) => self.set_dir(dir.clone()),
            None => self.viewing = Some(i - listing.dirs.len()),
        }
    }

    /// Rewrites the thumbnail queue when the visible rows or the selection change.
    /// Priority: the selected image, then the visible ones from top to bottom, then the nearby
    /// rows, then all the others in the folder from top to bottom (saved to disk only).
    /// `up`: 1 if the grid starts with the ".." cell.
    pub(crate) fn prefetch_thumbs(&mut self, rows: std::ops::Range<usize>, cols: usize, up: usize, ndirs: usize, nfiles: usize) {
        let (Some(loader), Some(Ok(listing))) = (&self.loader, self.listings.get(&self.dir)) else { return };
        let to_file = |cell: usize| cell.saturating_sub(up + ndirs).min(nfiles);
        let visible = to_file(rows.start * cols)..to_file(rows.end * cols);
        let near = to_file(rows.start.saturating_sub(MARGIN_ROWS) * cols)..to_file((rows.end + MARGIN_ROWS) * cols);
        let end = (visible.start.saturating_sub(MAX_THUMBS / 2) + MAX_THUMBS).min(nfiles);
        let keep = end.saturating_sub(MAX_THUMBS)..end;
        self.thumbs.retain(|i, _| keep.contains(i));
        self.thumb_keep = keep.clone();

        let selected = self.selected.checked_sub(ndirs).filter(|&i| i < nfiles);
        let key = (visible.clone(), selected, self.generation);
        if self.thumb_queue_key.as_ref() == Some(&key) {
            return;
        }
        let order = thumb_order(selected, visible, near, keep, nfiles, |i| self.thumbs.contains_key(&i), |i| {
            self.generated.contains(&i)
        });
        loader.set_thumb_queue(self.generation, order.into_iter().map(|i| (i, listing.files[i].clone())).collect());
        self.thumb_queue_key = Some(key);
    }
}

/// Order in which thumbnails are generated: selected, visible, nearby rows, the rest of those to
/// keep in memory (even if already on disk, they must be reloaded), then the rest of the folder
/// not yet generated.
fn thumb_order(
    selected: Option<usize>,
    visible: std::ops::Range<usize>,
    near: std::ops::Range<usize>,
    keep: std::ops::Range<usize>,
    nfiles: usize,
    in_memory: impl Fn(usize) -> bool,
    generated: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut queued = HashSet::new();
    let near = selected.into_iter().chain(visible).chain(near).chain(keep);
    near.chain((0..nfiles).filter(|&i| !generated(i)))
        .filter(|&i| !in_memory(i) && queued.insert(i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::thumb_order;

    #[test]
    pub(crate) fn order() {
        // 12 images, visible 4..8, nearby 2..10, in memory 1..11, number 9 selected, number 5
        // already in memory, 0 and 11 already on disk.
        let order = thumb_order(Some(9), 4..8, 2..10, 1..11, 12, |i| i == 5, |i| i == 0 || i == 11);
        assert_eq!(order, vec![9, 4, 6, 7, 2, 3, 8, 1, 10]);
    }
}
