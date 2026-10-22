//! Browse mode: folder tree on the left, thumbnail grid on the right.

use eframe::egui::{
    self, Align2, Color32, FontId, Key, Label, Rect, ScrollArea, Sense, Ui, collapsing_header::CollapsingState,
    pos2, vec2,
};

use std::collections::HashSet;

use crate::devices::Device;
use crate::meta::Group;
use crate::{Act, App, Focus, Go};
use crate::source;

/// Thumbnail rows loaded right after the visible ones, above and below.
const MARGIN_ROWS: usize = 3;
/// Thumbnails kept in memory, in a window around the visible ones: beyond it, when scrolling, the
/// distant ones are reloaded from disk and disappear for a moment. At most 256×256 RGBA, so
/// about 250 MB in the worst case; a smaller folder stays entirely in memory.
const MAX_THUMBS: usize = 1000;

/// Selection colour: full in the part receiving the keyboard, faint in the other, so the two
/// selections (folder open on the left, item on the right) do not look in conflict.
fn selection_fill(ui: &Ui, active: bool) -> Color32 {
    let fill = ui.visuals().selection.bg_fill;
    if active { fill } else { fill.gamma_multiply(0.45) }
}

/// Frame around the part that receives arrows and Tab.
fn focus_frame(ui: &Ui) {
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
            items.push(Some((t("menu-rename"), "F2".into(), Act::Rename(path.clone()))));
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

/// Item without an arrow, aligned with the tree nodes that have one.
fn leaf(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_space(ui.spacing().icon_width);
        ui.selectable_label(false, text)
    })
    .inner
}

impl App {
    /// External devices (if any) above the source's tree. The titles of the two blocks stay
    /// still: each block has its own scroll area.
    pub(crate) fn tree(&mut self, ui: &mut Ui) {
        // A click on a device then corrects it to `Focus::Devices`.
        if ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(ui.max_rect()) {
            self.focus = Focus::Tree;
        }
        self.tree_rows.clear();
        self.tree_blocks(ui);
        if self.focus != Focus::Grid {
            focus_frame(ui);
        }
        // Once drawing is finished, as for the grid.
        match self.focus {
            Focus::Tree => self.tree_keys(ui.ctx()),
            Focus::Devices(i) => self.devices_keys(ui.ctx(), i),
            Focus::Grid => {}
        }
    }

    /// Devices of the Devices block, in the order they appear.
    fn external(&self) -> Vec<Device> {
        self.devices.iter().filter(|d| !d.internal).cloned().collect()
    }

    /// The desktop-mounted phone whose root is `dir`. If the root is empty, the phone was locked
    /// or not in "File transfer" when it was connected (gvfs: "Get Storage information failed").
    pub(crate) fn phone_root(&self, dir: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.phone && d.path.trim_start_matches('/') == dir)
    }

    /// What a click on a device does.
    fn open_device(&mut self, ctx: &egui::Context, d: &Device) {
        if let Some(block) = &d.block {
            crate::devices::mount(block.clone(), self.tx.clone(), ctx.clone());
        } else if d.path == self.base {
            self.set_dir(String::new());
        } else if self.base == "/" && d.path.starts_with('/') {
            self.set_dir(d.path.trim_start_matches('/').to_string());
        } else {
            self.open_source(ctx, &d.path);
        }
    }

    /// In the Devices block: up and down change the highlighted device, Enter opens it.
    fn devices_keys(&mut self, ctx: &egui::Context, i: usize) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (down, up, enter, right, left) = ctx.input(|k| {
            let p = |key| k.key_pressed(key);
            (p(Key::ArrowDown), p(Key::ArrowUp), p(Key::Enter), p(Key::ArrowRight), p(Key::ArrowLeft))
        });
        let external = self.external();
        // The device in the block, if it is a tree: row and state (open or closed).
        let node = external.get(i).and_then(|d| {
            let dir = if d.path == self.base { String::new() } else { d.path.trim_start_matches('/').to_string() };
            let at = self.tree_rows[..self.tree_split].iter().position(|(_, r)| *r == dir)?;
            Some((at, CollapsingState::load(ctx, self.tree_rows[at].0)?))
        });
        let open = node.as_ref().is_some_and(|(_, s)| s.is_open());
        // Right and left open and close it.
        if (right || left) && let Some((_, mut state)) = node {
            state.set_open(right);
            state.store(ctx);
        } else if down && open && let Some((at, _)) = node {
            // Open: down enters its first folder and the arrows move to the tree.
            let (_, dir) = &self.tree_rows[at];
            if let Some((_, child)) = self.tree_rows.get(at + 1).filter(|(_, c)| c.starts_with(dir.as_str()) && c != dir) {
                let child = child.clone();
                self.focus = Focus::Tree;
                self.set_dir(child);
            }
        } else if down && i + 1 < external.len() {
            self.focus = Focus::Devices(i + 1);
            self.scroll_to_device = true;
        } else if up && i > 0 {
            self.focus = Focus::Devices(i - 1);
            self.scroll_to_device = true;
        } else if enter && let Some(d) = external.get(i) {
            self.open_device(ctx, d);
        }
    }

    /// Tab: next image in the grid, then the devices one by one, then the folders of the
    /// computer (or of the open device) one by one, then the grid again from the first item.
    /// Shift+Tab (`back`) goes round the same way in reverse.
    pub(crate) fn tab(&mut self, ctx: &egui::Context, back: bool) {
        let ndev = self.external().len();
        let n = self.items();
        let rows = if self.tree_split < self.tree_rows.len() { self.tree_split } else { 0 };
        let rows: Vec<String> = self.tree_rows[rows..].iter().map(|(_, d)| d.clone()).collect();
        let at = rows.iter().position(|d| *d == self.dir);
        // In the tree, stay on the open folder if there is one; otherwise the first (or the last
        // going backwards).
        let to_tree = |app: &mut Self| {
            app.focus = Focus::Tree;
            let end = if back { rows.last() } else { rows.first() };
            if at.is_none() && let Some(d) = end {
                app.set_dir(d.clone());
            }
        };
        let to_devices = |app: &mut Self, i: usize| {
            app.focus = Focus::Devices(i);
            app.scroll_to_device = true;
        };
        let to_grid = |app: &mut Self| {
            app.focus = Focus::Grid;
            app.selected = if back { n.saturating_sub(1) } else { 0 };
            app.cursor = n > 0;
            app.scroll_to_selected = true;
        };
        match (self.focus, back) {
            (Focus::Grid, false) if n > 0 && (!self.cursor || self.selected + 1 < n) => self.go(Go::Next),
            (Focus::Grid, false) if ndev > 0 => to_devices(self, 0),
            (Focus::Grid, false) => to_tree(self),
            (Focus::Devices(i), false) if i + 1 < ndev => to_devices(self, i + 1),
            (Focus::Devices(_), false) => to_tree(self),
            (Focus::Tree, false) => match at {
                Some(at) if at + 1 < rows.len() => self.set_dir(rows[at + 1].clone()),
                _ => to_grid(self),
            },
            (Focus::Grid, true) if self.cursor && self.selected > 0 => self.go(Go::Prev),
            (Focus::Grid, true) => to_tree(self),
            (Focus::Tree, true) => match at {
                Some(at) if at > 0 => self.set_dir(rows[at - 1].clone()),
                _ if ndev > 0 => to_devices(self, ndev - 1),
                _ => to_grid(self),
            },
            (Focus::Devices(i), true) if i > 0 => to_devices(self, i - 1),
            (Focus::Devices(_), true) => to_grid(self),
        }
        ctx.request_repaint();
    }

    /// Arrows in the tree: down and up move to the next or previous node; right opens the node,
    /// or if it is already open goes to the first subfolder; left closes it, or if closed goes up.
    fn tree_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (down, up, right, left, back) = ctx.input(|i| {
            let k = |key| i.key_pressed(key);
            (k(Key::ArrowDown), k(Key::ArrowUp), k(Key::ArrowRight), k(Key::ArrowLeft), k(Key::Backspace))
        });
        if back {
            self.go_up();
            return;
        }
        let Some(at) = self.tree_rows.iter().position(|(_, d)| *d == self.dir) else { return };
        let (id, dir) = self.tree_rows[at].clone();
        let mut state = CollapsingState::load(ctx, id);
        let open = state.as_ref().is_some_and(|s| s.is_open());
        let next = self.tree_rows.get(at + 1).map(|(_, d)| d.clone());
        let target = if down {
            next
        } else if up {
            at.checked_sub(1).map(|i| self.tree_rows[i].1.clone())
        } else if right && !open {
            if let Some(s) = &mut state {
                s.set_open(true);
            }
            None
        } else if right {
            next.filter(|d| d.starts_with(&dir) && *d != dir)
        } else if left && open {
            if let Some(s) = &mut state {
                s.set_open(false);
            }
            None
        } else if left {
            let parent = source::parent(&dir);
            (!dir.is_empty() && self.tree_rows.iter().any(|(_, d)| *d == parent)).then_some(parent)
        } else {
            None
        };
        if let Some(s) = state {
            s.store(ctx);
        }
        if let Some(d) = target {
            self.set_dir(d);
        }
    }

    fn tree_blocks(&mut self, ui: &mut Ui) {
        let local = self.base == "/";
        // On Windows the internal drives go under "This computer", in drive letter order.
        let (mut internal, external): (Vec<_>, Vec<_>) = self.devices.iter().cloned().partition(|d| d.internal);
        internal.sort_by(|a, b| a.path.cmp(&b.path));
        if !external.is_empty() {
            ui.strong(self.i18n.t("devices"));
            // At most half of the panel, the rest goes to the computer's tree.
            let max = ui.available_height() * 0.5;
            ScrollArea::both().id_salt("devices").max_height(max).auto_shrink([false, true]).show(ui, |ui| {
                // Mounted devices are local folders: with the local source they are browsed in
                // the tree; otherwise a click opens the device as a source (also the directly
                // connected ones, `mtp://`), and from there it is browsed here.
                for (i, d) in external.iter().enumerate() {
                    let icon = if d.phone { "📱" } else { "🖴" };
                    let label = format!("{icon} {}", d.name);
                    let (rect, clicked) = if let Some(block) = &d.block {
                        // Not mounted: dimmed name, a click mounts it.
                        let hint = self.i18n.t_args("device-mount-hint", &[("device", block)]);
                        let r = leaf(ui, egui::RichText::new(label).weak()).on_hover_text(hint);
                        (r.rect, r.clicked())
                    } else if d.path == self.base {
                        self.tree_node(ui, "device", String::new(), &label, true)
                    } else if local && d.path.starts_with('/') {
                        let dir = d.path.trim_start_matches('/').to_string();
                        self.tree_node(ui, "device", dir, &label, true)
                    } else {
                        let r = leaf(ui, label);
                        (r.rect, r.clicked())
                    };
                    if clicked {
                        self.focus = Focus::Devices(i);
                        // The tree node has already opened its folder.
                        if d.block.is_some() || !(d.path == self.base || local && d.path.starts_with('/')) {
                            let ctx = ui.ctx().clone();
                            self.open_device(&ctx, d);
                        }
                    }
                    if self.focus == Focus::Devices(i) {
                        let stroke = ui.visuals().selection.stroke;
                        ui.painter().rect_stroke(rect.expand(1.0), 2.0, stroke, egui::StrokeKind::Outside);
                        // Only when arriving from the keyboard: on every frame the mouse wheel
                        // could not scroll away from the device.
                        if std::mem::take(&mut self.scroll_to_device) {
                            ui.scroll_to_rect(rect, None);
                        }
                    }
                }
            });
            ui.separator();
        }
        self.tree_split = self.tree_rows.len();
        // With a device open the tree would be a duplicate: in its place, the way back home.
        // The way back is also needed if the open phone was unplugged (its error stays below).
        let device_open = external.iter().any(|d| d.path == self.base);
        if device_open || self.base.starts_with("mtp://") {
            let home = std::env::home_dir().map_or("/".into(), |h| h.to_string_lossy().into_owned());
            if ui.selectable_label(false, format!("🏠 {home}")).clicked() {
                let ctx = ui.ctx().clone();
                self.open_source(&ctx, &home);
            }
            if device_open {
                return;
            }
        }
        // The current folder is shown open in one block only: if it is inside a device,
        // the main tree does not open all the way to /run/user/…
        let in_device = local
            && self.devices.iter().any(|d| d.path.starts_with('/') && self.dir.starts_with(d.path.trim_start_matches('/')));
        let base = self.base.clone();
        if !base.contains("://") {
            ui.strong(self.i18n.t("this-computer"));
        }
        ScrollArea::both().id_salt("tree").auto_shrink(false).show(ui, |ui| {
            // Tree of the open source; the other internal drives open with a click.
            if !internal.iter().any(|d| d.path == base) {
                self.tree_node(ui, "tree", String::new(), &format!("🗀 {base}"), !in_device);
            }
            for d in &internal {
                if d.path == base {
                    self.tree_node(ui, "tree", String::new(), &format!("🖴 {}", d.name), !in_device);
                } else if leaf(ui, format!("🖴 {}", d.name)).clicked() {
                    let ctx = ui.ctx().clone();
                    self.open_source(&ctx, &d.path);
                }
            }
        });
    }

    /// `salt` tells apart the same path shown in two blocks (devices and tree).
    /// `may_reveal`: this block may open by itself down to the current folder.
    /// Returns the rectangle of the name and whether it was clicked.
    fn tree_node(&mut self, ui: &mut Ui, salt: &str, dir: String, label: &str, may_reveal: bool) -> (Rect, bool) {
        let id = ui.make_persistent_id((salt, &dir));
        self.tree_rows.push((id, dir.clone()));
        let on_path = self.dir.starts_with(&dir);
        // Open at the start: the ancestors of the current folder, only in the block containing it
        // (the root always).
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, on_path && (may_reveal || dir.is_empty()));
        // After a folder change (also from the grid) the ancestors reopen until the folder has
        // been shown; then they can be closed by hand. Not the folder itself: going down with
        // the arrows would open every node instead of moving to the next one.
        let selected = self.dir == dir;
        let reveal = may_reveal && self.reveal && on_path;
        if reveal && !selected {
            state.set_open(true);
        }
        let mut clicked = false;
        let mut rect = Rect::NOTHING;
        let header = state.show_header(ui, |ui| {
            ui.visuals_mut().selection.bg_fill = selection_fill(ui, self.focus == Focus::Tree);
            let response = ui.selectable_label(selected, label);
            rect = response.rect;
            if reveal && selected {
                response.scroll_to_me(Some(egui::Align::Center));
                self.reveal = false;
            }
            if response.clicked() {
                self.set_dir(dir.clone());
                clicked = true;
            }
            response.context_menu(|ui| {
                let items = self.item_menu(Some(dir.clone()), Some(Act::OpenDir(dir.clone())));
                if let Some(a) = context_menu(ui, items) {
                    self.act = Some(a);
                }
            });
        });
        header.body(|ui| match self.listings.get(&dir) {
            Some(Ok(listing)) => {
                for child in listing.dirs.clone() {
                    let icon = if source::is_trash(&child) { "🗑" } else { "🗀" };
                    let label = format!("{icon} {}", self.item_name(&child));
                    self.tree_node(ui, salt, child, &label, may_reveal);
                }
            }
            Some(Err(e)) => {
                ui.label(error_text(ui, e.as_str()));
            }
            None => {
                self.request_listing(&dir);
                ui.spinner();
            }
        });
        // A click on the name also opens the node (the arrows do not, see above).
        if clicked && let Some(mut state) = CollapsingState::load(ui.ctx(), id) {
            state.set_open(true);
            state.store(ui.ctx());
        }
        (rect, clicked)
    }

    pub(crate) fn grid(&mut self, ui: &mut Ui) {
        if ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(ui.max_rect()) {
            self.focus = Focus::Grid;
        }
        // Right click on the empty space: the actions of the whole folder. The cells come later
        // and are on top, so they keep their own menu.
        let background = ui.interact(ui.max_rect(), ui.scope_id().with("grid-background"), Sense::click());
        background.context_menu(|ui| {
            let t = |id| self.i18n.t(id);
            let mut items = vec![Some((t("menu-refresh"), "F5".into(), Act::Refresh))];
            if !self.dir.is_empty() {
                items.push(Some((t("menu-parent"), t("key-backspace"), Act::Up)));
            }
            if let Some(a) = context_menu(ui, items) {
                self.act = Some(a);
            }
        });
        if self.focus == Focus::Grid {
            focus_frame(ui);
        }
        let Some(Ok(listing)) = self.listings.get(&self.dir) else {
            ui.centered_and_justified(|ui| match self.listings.get(&self.dir) {
                Some(Err(e)) => ui.label(error_text(ui, e.as_str()).size(16.0)),
                _ => ui.spinner(),
            });
            return;
        };
        let (ndirs, nfiles) = (listing.dirs.len(), listing.files.len());
        let n = ndirs + nfiles;
        if n == 0 {
            let text = match self.phone_root(&self.dir) {
                Some(d) => error_text(ui, self.i18n.t_args("error-no-storage", &[("detail", &d.name)])).size(16.0),
                None => egui::RichText::new(self.i18n.t("folder-empty")),
            };
            ui.centered_and_justified(|ui| ui.label(text));
            // Backspace goes up from an empty folder too.
            if self.focus == Focus::Grid && !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Backspace)) {
                self.go_up();
            }
            return;
        }

        let side = self.thumb_size;
        let cell = vec2(side + 16.0, side + 32.0);
        let cols = ((ui.available_width() / cell.x).floor() as usize).max(1);
        let rows = n.div_ceil(cols);

        let mut visible_rows = 0..0;
        // Double click and keys (Enter, Backspace) run once the grid is finished: changing folder
        // before or during drawing, the cells would read the new listing with the old counts
        // (crash of 2026-10-20 when re-entering a folder already visited).
        let mut open = None;
        // Bar always visible and wide: the thin one that widens on hover was hard to grab. If
        // egui loses the drag (see `BarDrag`), `bar_offset` continues it.
        // Handle grey like the weak text, darker under the mouse: with the button colour it was
        // barely distinguishable from the light background.
        let mut scroll = egui::style::ScrollStyle::solid();
        scroll.bar_width = 10.0;
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
            let cell_rect = |i: usize| {
                let pos = origin + vec2((i % cols) as f32 * cell.x, (i / cols) as f32 * cell.y);
                Rect::from_min_size(pos, cell)
            };
            let first = (viewport.min.y / cell.y).floor().max(0.0) as usize;
            let last = ((viewport.max.y / cell.y).ceil() as usize).min(rows);
            visible_rows = first..last;
            for i in first * cols..(last * cols).min(n) {
                if self.grid_cell(ui, cell_rect(i), i, ndirs) {
                    open = Some(i);
                }
            }
            if std::mem::take(&mut self.scroll_to_selected) {
                ui.scroll_to_rect(cell_rect(self.selected), None);
            }
        });
        self.grid_rect = Some(out.inner_rect);
        self.track_bar(ui, out.inner_rect, content, out.state.clamped_offset().y);
        self.prefetch_thumbs(visible_rows, cols, ndirs, nfiles);
        if let Some(i) = open {
            self.activate(i);
        } else if self.focus == Focus::Grid {
            self.grid_keys(ui.ctx(), cols, n);
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

    /// Metadata boxes of the selected image, side by side below the grid; each one closes with
    /// its X and reopens from the View menu or the ℹ button.
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

    /// Draws a cell; returns `true` if it was double-clicked.
    fn grid_cell(&mut self, ui: &mut Ui, rect: Rect, i: usize, ndirs: usize) -> bool {
        let Some(Ok(listing)) = self.listings.get(&self.dir) else { return false };
        let path = if i < ndirs { listing.dirs[i].clone() } else { listing.files[i - ndirs].path.clone() };
        let name = self.item_name(&path);
        let response = ui.interact(rect, ui.scope_id().with(("cell", i)), Sense::click()).on_hover_text(&name);

        let painter = ui.painter_at(rect);
        let active = self.focus == Focus::Grid;
        let fill = selection_fill(ui, active);
        let visuals = ui.visuals();
        let selected = self.cursor && i == self.selected;
        if selected {
            painter.rect_filled(rect.shrink(2.0), 6.0, fill);
        } else if response.hovered() {
            painter.rect_filled(rect.shrink(2.0), 6.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let side = self.thumb_size;
        let thumb_rect = Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(side, side));
        let dim = visuals.weak_text_color();
        if i < ndirs {
            let icon = if source::is_trash(&path) { "🗑" } else { "🗀" };
            painter.text(thumb_rect.center(), Align2::CENTER_CENTER, icon, FontId::proportional(side * 0.5), dim);
        } else {
            // Every image in its own tile, so you can see where one photo ends and the next begins
            // (not folders: they stay recognisable at a glance). Selected: solid border.
            let (tile, tile_stroke) = crate::theme::tile(visuals);
            let accent = crate::theme::accent(visuals);
            let stroke = match (selected, active) {
                (true, true) => egui::Stroke::new(2.0, accent),
                (true, false) => egui::Stroke::new(1.5, accent.gamma_multiply(0.6)),
                _ => egui::Stroke::new(1.0, tile_stroke),
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
        let label_rect = Rect::from_min_size(pos2(rect.min.x + 4.0, thumb_rect.max.y + 7.0), vec2(rect.width() - 8.0, 18.0));
        // Image names brighter than folder names (they used to be all grey on grey).
        let color = if i < ndirs { visuals.text_color() } else { visuals.text_color().lerp_to_gamma(visuals.strong_text_color(), 0.6) };
        ui.put(label_rect, Label::new(egui::RichText::new(name).color(color)).truncate().selectable(false));

        if response.clicked() {
            self.selected = i;
            self.cursor = true;
        }
        // The right click selects the item too, so the menu acts on it.
        if response.secondary_clicked() {
            self.selected = i;
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

    fn grid_keys(&mut self, ctx: &egui::Context, cols: usize, n: usize) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let before = (self.selected, self.cursor);
        let cursor = self.cursor;
        let mut activate = false;
        let mut up = false;
        let mut moves = Vec::new();
        ctx.input(|i| {
            for (key, to) in [(Key::ArrowRight, Go::Next), (Key::ArrowLeft, Go::Prev), (Key::Home, Go::First), (Key::End, Go::Last)] {
                if i.key_pressed(key) {
                    moves.push(to);
                }
            }
            let s = &mut self.selected;
            // With no selection shown, down and up start from the first item.
            if i.key_pressed(Key::ArrowDown) {
                *s = if cursor { (*s + cols).min(n - 1) } else { 0 };
                self.cursor = true;
            }
            if i.key_pressed(Key::ArrowUp) {
                *s = if cursor { s.saturating_sub(cols) } else { 0 };
                self.cursor = true;
            }
            activate = i.key_pressed(Key::Enter) && cursor;
            up = i.key_pressed(Key::Backspace);
        });
        for to in moves {
            self.go(to);
        }
        self.selected = self.selected.min(n - 1);
        if (self.selected, self.cursor) != before {
            self.scroll_to_selected = true;
        }
        if activate {
            self.activate(self.selected);
        } else if up {
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
    fn prefetch_thumbs(&mut self, rows: std::ops::Range<usize>, cols: usize, ndirs: usize, nfiles: usize) {
        let (Some(loader), Some(Ok(listing))) = (&self.loader, self.listings.get(&self.dir)) else { return };
        let to_file = |cell: usize| cell.saturating_sub(ndirs).min(nfiles);
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
