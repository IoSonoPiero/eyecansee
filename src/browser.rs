//! Browse mode: folder tree on the left, thumbnail grid on the right.

use eframe::egui::{self, Align2, Color32, FontId, Key, Label, Rect, ScrollArea, Sense, Ui, pos2, vec2};

use std::collections::HashSet;

use crate::icons::{self, Icon};
use crate::meta::Group;
use crate::actions::Act;
use crate::{App, Focus, meta};
use crate::source;

/// Thumbnail rows loaded right after the visible ones, above and below.
const MARGIN_ROWS: usize = 3;
/// Thumbnails kept in memory, in a window around the visible ones: beyond it, when scrolling, the
/// distant ones are reloaded from disk and disappear for a moment. At most 256×256 RGBA, so
/// about 250 MB in the worst case; a smaller folder stays entirely in memory.
const MAX_THUMBS: usize = 1000;

/// Selection colour: full in the part receiving the keyboard, faint in the other, so the two
/// selections (folder open on the left, item on the right) do not look in conflict.
pub(crate) fn selection_fill(ui: &Ui, active: bool) -> Color32 {
    let fill = ui.visuals().selection.bg_fill;
    if active { fill } else { fill.gamma_multiply(0.45) }
}

/// Frame around the part that receives arrows and Tab.
pub(crate) fn focus_frame(ui: &Ui) {
    let stroke = egui::Stroke::new(1.0, crate::theme::accent(ui.visuals()));
    ui.painter().rect_stroke(ui.max_rect(), 2.0, stroke, egui::StrokeKind::Outside);
}

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

/// Text of an error or warning: the former light red was barely readable on the light background.
/// Item of a right-click menu: (label, shortcut, action); `None` = separator.
pub(crate) type MenuItem = Option<(String, String, Act)>;

/// Right-click menu; returns the chosen item, which the caller runs at the end of the frame (`App::act`).
pub(crate) fn context_menu(ui: &mut Ui, items: Vec<MenuItem>) -> Option<Act> {
    let mut chosen = None;
    for item in items {
        match item {
            Some((label, _, Act::Submenu(sub))) => {
                ui.menu_button(label, |ui| {
                    if let Some(a) = context_menu(ui, sub) {
                        chosen = Some(a);
                    }
                });
            }
            Some((label, keys, act)) => {
                if ui.add(egui::Button::new(label).shortcut_text(keys)).clicked() {
                    chosen = Some(act);
                }
            }
            None => {
                ui.separator();
            }
        }
    }
    chosen
}

impl App {
    /// Right-click menu of an item, as in file managers: open first, the deletions last and
    /// apart, so a slip of the mouse does not hit them. `path`: `None` = the selected item or
    /// the one in the viewer; `open`: the action that opens it (`None` in the viewer).
    pub(crate) fn item_menu(&self, path: Option<String>, open: Option<Act>) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let in_viewer = open.is_none();
        let mut items = vec![match open {
            Some(a @ Act::Open) => Some((t("menu-open-selected"), t("key-enter"), a)),
            Some(a) => Some((t("menu-open-selected"), String::new(), a)),
            None => Some((t("menu-back-to-grid"), "Esc".into(), Act::Back)),
        }];
        let Some(target) = path.clone().or_else(|| self.target()).filter(|p| self.can_change(p)) else { return items };
        // The trash folder: only put everything back or empty it. What is inside it: look at it,
        // put it back or delete it for good (Del does that too there).
        if source::is_trash(&target) {
            items.push(Some((t("menu-restore-all"), String::new(), Act::Restore(path.clone()))));
            items.push(None);
            items.push(Some((t("menu-empty-trash"), t("key-delete"), Act::Delete(path, true))));
        } else if source::in_trash(&target) {
            items.push(Some((t("menu-restore"), String::new(), Act::Restore(path.clone()))));
            items.push(None);
            items.push(Some((t("menu-delete-permanent"), t("key-delete"), Act::Delete(path, true))));
        } else {
            // As in file managers: cut, copy, paste, then rename.
            items.push(None);
            items.push(Some((t("menu-cut"), t("key-cut"), Act::Cut(path.clone()))));
            items.push(Some((t("menu-copy"), t("key-copy"), Act::Copy(path.clone()))));
            // Not into one of the folders being copied: it would go inside itself.
            let copied = self.clip.as_ref().is_some_and(|c| c.base == self.base && c.paths.contains(&target));
            if target.ends_with('/') && !copied && self.can_paste(&target) {
                items.push(Some((t("menu-paste-into"), String::new(), Act::Paste(Some(target.clone())))));
            }
            items.push(None);
            items.push(Some((t("menu-rename"), "F2".into(), Act::Rename(path.clone()))));
            // Flips and turns: in the viewer shown until saved, in the grid saved straight away.
            if !target.ends_with('/') {
                items.push(None);
                let mut turns = Vec::new();
                for (op, label, keys) in crate::actions::transform_items() {
                    turns.push(Some((t(label), if in_viewer { keys.map(t).unwrap_or_default() } else { String::new() }, Act::Transform(op, path.clone()))));
                    if op == crate::transform::Op::FlipV {
                        turns.push(None);
                    }
                }
                items.push(Some((t("menu-transform"), String::new(), Act::Submenu(turns))));
                if in_viewer && self.can_save() {
                    items.push(Some((t("menu-save"), t("key-save"), Act::Save)));
                }
                items.push(Some((t("menu-save-copy"), String::new(), Act::Convert(path.clone()))));
                items.push(Some((t("menu-redo-thumbs"), String::new(), Act::RedoThumbs(path.clone()))));
            }
            items.push(None);
            items.push(Some((t("menu-delete"), t("key-delete"), Act::Delete(path.clone(), false))));
            items.push(Some((t("menu-delete-permanent"), t("key-shift-delete"), Act::Delete(path, true))));
        }
        items
    }

    /// Name shown for a folder or image: trash folders have their own, in the user's language.
    pub(crate) fn item_name(&self, path: &str) -> String {
        if source::is_trash(path) { self.i18n.t("trash") } else { source::name(path).to_string() }
    }
}

pub(crate) fn error_text(ui: &Ui, text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(ui.visuals().error_fg_color)
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
    fn track_bar(&mut self, ui: &Ui, rect: Rect, content: f32, offset: f32) {
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
    fn bar_offset(&self, ui: &Ui, rect: Rect, content: f32) -> Option<f32> {
        let d = self.bar_drag.as_ref().filter(|d| d.lost)?;
        let p = ui.input(|i| i.pointer.latest_pos())?;
        let max = (content - rect.height()).max(0.0);
        let travel = rect.height() - handle_len(ui, rect, content);
        if max <= 0.0 || travel <= 0.0 {
            return None;
        }
        Some(((p.y - d.grab - rect.top()) / travel * max).clamp(0.0, max))
    }

    pub(crate) fn meta_shown(&self, g: meta::Group) -> bool {
        !self.settings.meta_hidden.iter().any(|k| k == g.key())
    }

    /// Opens or closes a metadata box (X, menu, button) and remembers it.
    pub(crate) fn show_meta(&mut self, g: meta::Group, show: bool) {
        self.settings.meta_hidden.retain(|k| k != g.key());
        if !show {
            self.settings.meta_hidden.push(g.key().to_string());
        }
        self.meta_height = 0.0;
        self.settings.save();
    }

    /// Metadata boxes of the selected image, side by side below the grid; each one closes with
    /// its X and reopens from the View menu or the ℹ button.
    #[allow(clippy::too_many_lines)] // to be split
    pub(crate) fn meta_panel(&mut self, ui: &mut Ui) {
        // Title bar: a click anywhere, or on the button on the right, collapses and reopens the
        // panel. Dragging its edge was useless: it always went back to the content's height.
        let collapsed = self.settings.meta_collapsed;
        let (rect, bar) = ui.allocate_exact_size(vec2(ui.available_width(), ui.spacing().interact_size.y), Sense::click());
        let font = egui::TextStyle::Body.resolve(ui.style());
        ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, self.i18n.t("meta-title"), font, ui.visuals().strong_text_color());
        let button_rect = Rect::from_min_max(pos2(rect.right() - 26.0, rect.top()), rect.max);
        let hint = self.i18n.t(if collapsed { "meta-expand" } else { "meta-collapse" });
        let button = ui.put(button_rect, egui::Button::new(if collapsed { "▲" } else { "▼" }).small()).on_hover_text(&hint);
        let bar = bar.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand);
        if bar.clicked() || button.clicked() {
            self.settings.meta_collapsed = !collapsed;
            log!("metadata collapsed: {}", !collapsed);
            self.settings.save();
        }
        if self.settings.meta_collapsed {
            return;
        }
        let ndirs = match self.listings.get(&self.dir) {
            Some(Ok(l)) => l.dirs.len(),
            _ => 0,
        };
        let file = self.selected.checked_sub(ndirs).filter(|_| self.cursor);
        let file = file.and_then(|i| self.files().get(i).map(|f| (i, f.path.clone())));
        if let (Some((i, path)), Some(loader)) = (&file, &self.loader)
            && self.meta_asked != Some((self.generation, *i))
        {
            loader.load_meta(self.generation, *i, path.clone());
            self.meta_asked = Some((self.generation, *i));
        }
        // While the new data arrives the previous image's stay: the values that do not change
        // (make, model, lens…) stay still instead of blinking through "Reading…".
        let meta = match (&file, &self.meta) {
            (Some(_), Some((_, m))) => Some(m.clone()),
            _ => None,
        };
        let t = |id: &str| self.i18n.t(id);
        let note = match (&file, &meta) {
            (None, _) => t("meta-none-selected"),
            (Some(_), None) => t("meta-loading"),
            (_, Some(Err(e))) => e.clone(),
            (_, Some(Ok(None))) => t("meta-no-exif"),
            _ => String::new(),
        };
        // The panel never shrinks: with a photo without EXIF, or while the data arrives (only
        // one line), it shrank and grew back at every click and the grid above jumped.
        // It starts again from its content when a box is opened or closed (`show_meta`).
        // Only the boxes are measured: the whole panel includes the title bar and grew at every frame.
        let meta = meta.and_then(|m| m.ok().flatten());
        let mut close = None;
        let min_height = self.meta_height;
        let boxes = ScrollArea::horizontal().show(ui, |ui| {
            ui.set_min_height(min_height);
            ui.horizontal_top(|ui| {
                for g in Group::ALL.into_iter().filter(|&g| self.meta_shown(g)) {
                    egui::Frame::group(ui.style()).show(ui, |ui| ui.vertical(|ui| {
                        ui.set_width(250.0);
                        ui.horizontal(|ui| {
                            ui.strong(t(&format!("meta-{}", g.key())));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✖").on_hover_text(t("meta-close")).clicked() {
                                    close = Some(g);
                                }
                            });
                        });
                        ui.separator();
                        let Some(m) = &meta else {
                            ui.weak(&note);
                            return;
                        };
                        let fields: Vec<_> = m.fields.iter().filter(|(fg, _, _)| *fg == g).collect();
                        let gps = m.gps.filter(|_| g == Group::Gps);
                        if fields.is_empty() && gps.is_none() {
                            ui.weak(t("meta-empty"));
                        }
                        egui::Grid::new(("meta", g.key())).num_columns(2).show(ui, |ui| {
                            if let Some((lat, lon)) = gps {
                                ui.weak(t("meta-coords"));
                                ui.label(format!("{lat:.6}, {lon:.6}"));
                                ui.end_row();
                            }
                            for (_, label, value) in fields {
                                ui.weak(t(label));
                                ui.label(self.meta_value(label, value));
                                ui.end_row();
                            }
                        });
                        if let Some((lat, lon)) = gps {
                            let url = format!("https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map=16/{lat}/{lon}");
                            ui.hyperlink_to(t("meta-map"), url);
                        }
                    }));
                }
            });
        });
        self.meta_height = self.meta_height.max(boxes.content_size.y);
        if let Some(g) = close {
            self.show_meta(g, false);
        }
    }

    /// Status bar of the viewer, like the grid's: name, position, pixels, size on disk, date
    /// taken (or modified, without EXIF or with the metadata boxes closed).
    pub(crate) fn viewer_status(&self, ui: &mut Ui, current: usize) {
        let files = self.files();
        let Some(f) = files.get(current) else { return };
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        ui.horizontal(|ui| {
            ui.label(source::name(&f.path));
            ui.separator();
            ui.label(format!("{} / {}", current + 1, files.len()));
            // shortcut: images beyond 8192 px show the size of their scaled-down texture; the decoder would have to report the real one.
            if let Some(Ok(tex)) = self.full.get(&current) {
                let [w, h] = tex.size();
                let (w, h) = if self.turn.sideways() { (h, w) } else { (w, h) };
                ui.separator();
                ui.label(self.i18n.t_args("convert-pixels", &[("w", &w.to_string()), ("h", &h.to_string())]));
                ui.separator();
                ui.label(format!("{:.0}%", self.zoom_shown * 100.0));
            }
            if !self.turn.is_none() {
                ui.separator();
                ui.label(self.i18n.t(if self.saving.contains(&f.path) { "status-saving" } else { "status-modified" }));
            }
            ui.separator();
            ui.label(self.human_size(f.size));
            let taken = match &self.meta {
                Some((i, Ok(Some(m)))) if *i == current => m.fields.iter().find(|(_, l, _)| *l == "meta-taken").map(|(_, l, v)| self.meta_value(l, v)),
                _ => None,
            };
            if let Some(date) = taken.or_else(|| self.file_date(&f.modified)) {
                ui.separator();
                ui.label(date);
            }
        });
    }

    /// Value of a metadata field as shown: Fluent ids translated, dates in the language's format.
    fn meta_value(&self, label: &str, value: &str) -> String {
        if value.starts_with("meta-") {
            return self.i18n.t(value);
        }
        let formatted = match label {
            "meta-taken" => value.parse::<jiff::civil::DateTime>().ok().map(|d| d.strftime(&self.i18n.t("date-format")).to_string()),
            "meta-gps-date" => value.parse::<jiff::civil::Date>().ok().map(|d| d.strftime(&self.i18n.t("date-day-format")).to_string()),
            _ => None,
        };
        formatted.unwrap_or_else(|| value.to_string())
    }

    /// Strip at the top of the grid, only where there is a trash: in a folder that has one, the
    /// button to open it; inside a trash, whose it is and the actions on all of it. The buttons
    /// run at the end of the frame (`App::act`).
    fn trash_bar(&mut self, ui: &mut Ui) {
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
    fn up_cell(&mut self, ui: &mut Ui, rect: Rect) -> bool {
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
    fn grid_cell(&mut self, ui: &mut Ui, rect: Rect, i: usize, ndirs: usize) -> bool {
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
    fn mark_range(&mut self, i: usize) {
        let (a, b) = (self.anchor.min(i), self.anchor.max(i));
        self.marked = (a..=b).filter_map(|k| self.item_path(k)).collect();
    }

    /// Arrows, Home, End, Enter, Backspace, Ctrl+A. They work on positions in the grid, the ".."
    /// cell (`up` = 1) included; Home goes to the first item, the ".." cell is reached with the
    /// arrows. With Shift they mark from the anchor, without it they clear the marks.
    fn grid_keys(&mut self, ctx: &egui::Context, cols: usize, n: usize, up: usize) {
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
    fn prefetch_thumbs(&mut self, rows: std::ops::Range<usize>, cols: usize, up: usize, ndirs: usize, nfiles: usize) {
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
    fn order() {
        // 12 images, visible 4..8, nearby 2..10, in memory 1..11, number 9 selected, number 5
        // already in memory, 0 and 11 already on disk.
        let order = thumb_order(Some(9), 4..8, 2..10, 1..11, 12, |i| i == 5, |i| i == 0 || i == 11);
        assert_eq!(order, vec![9, 4, 6, 7, 2, 3, 8, 1, 10]);
    }
}
