//! Left panel: the Devices block and the folder tree of "This computer", with their keys and Tab
//! between the parts of the window.

use eframe::egui::{self, Color32, Key, Rect, ScrollArea, Ui, collapsing_header::CollapsingState};

use crate::actions::Act;
use crate::browser::{context_menu, error_text, focus_frame, selection_fill};
use crate::devices::Device;
use crate::icons::{self, Icon};
use crate::navigation::Go;
use crate::{App, Focus};
use crate::source;

/// Side of the icons in the left panel, in points.
const TREE_ICON: f32 = 16.0;

/// Item without an arrow, aligned with the tree nodes that have one.
fn leaf(ui: &mut Ui, icon: Icon, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_space(ui.spacing().icon_width);
        ui.selectable_label(false, (icons::image(ui.ctx(), icon, TREE_ICON), text.into()))
    })
    .inner
}

/// Title of a block of the left panel: small grey capitals, so it does not look like an item.
fn block_title(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text.to_uppercase()).size(11.0).strong().color(ui.visuals().weak_text_color()));
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
    pub(crate) fn external(&self) -> Vec<Device> {
        self.devices.iter().filter(|d| !d.internal).cloned().collect()
    }

    /// The desktop-mounted phone whose root is `dir`. If the root is empty, the phone was locked
    /// or not in "File transfer" when it was connected (gvfs: "Get Storage information failed").
    pub(crate) fn phone_root(&self, dir: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.phone && d.path.trim_start_matches('/') == dir)
    }

    /// What a click on a device does.
    pub(crate) fn open_device(&mut self, ctx: &egui::Context, d: &Device) {
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
    pub(crate) fn devices_keys(&mut self, ctx: &egui::Context, i: usize) {
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
    pub(crate) fn tree_keys(&mut self, ctx: &egui::Context) {
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

    /// The saved servers (File > Server manager): a click opens one, the open one is highlighted;
    /// right click: close its connection, edit it.
    fn server_block(&mut self, ui: &mut Ui) {
        // Encrypted and not unlocked yet: one row asking for the master password.
        if self.saved_servers.is_empty() && crate::servers::locked() {
            block_title(ui, &self.i18n.t("servers"));
            if leaf(ui, Icon::Server, self.i18n.t("vault-unlock-row")).clicked() {
                self.ask_master(crate::vault_page::Then::Panel);
            }
            ui.separator();
            return;
        }
        if self.saved_servers.is_empty() {
            return;
        }
        block_title(ui, &self.i18n.t("servers"));
        let (mut open, mut close, mut edit) = (None, None, None);
        for (i, s) in self.saved_servers.iter().enumerate() {
            let Some(address) = s.address() else { continue };
            let here = self.base.contains("://") && address.starts_with(&self.base);
            let ftp_port = s.port.unwrap_or(21);
            let live = here
                || s.scheme == "sftp" && crate::sftp::connected(&s.host, s.port, &s.user)
                || (s.scheme == "ftp" || s.scheme == "ftps") && crate::ftp::connected(&s.host, ftp_port, &s.user);
            let icon = icons::image(ui.ctx(), Icon::Server, TREE_ICON);
            let r = ui.selectable_label(here, (icon, s.name.as_str())).on_hover_text(crate::address::without_password(&address));
            if r.clicked() {
                open = Some(self.back_to(&address));
            }
            r.context_menu(|ui| {
                // Not during a copy: it could be using this connection.
                if ui.add_enabled(live && self.transfer.is_none(), egui::Button::new(self.i18n.t("srv-disconnect"))).clicked() {
                    close = Some((here, s.clone()));
                }
                if ui.button(self.i18n.t("srv-edit")).clicked() {
                    edit = Some(i);
                }
            });
        }
        ui.separator();
        let ctx = ui.ctx().clone();
        if let Some(address) = open {
            self.open_source(&ctx, &address);
        }
        if let Some((here, s)) = close {
            // Away from the server first, or the next listing would connect again.
            if here {
                let home = std::env::home_dir().map_or("/".into(), |h| h.to_string_lossy().into_owned());
                self.open_source(&ctx, &home);
            }
            if s.scheme == "ftp" || s.scheme == "ftps" {
                self.rt.spawn(crate::ftp::disconnect(s.host, s.port.unwrap_or(21), s.user));
            } else {
                self.rt.spawn(crate::sftp::disconnect(s.host, s.port, s.user));
            }
        }
        if let Some(i) = edit {
            let mut f = crate::address::AddressForm::manager();
            f.choose(Some(i));
            self.server_manager = Some(f);
        }
    }

    /// "This computer" with a phone or a server open: home and, with a server, the drives.
    fn way_home(&mut self, ui: &mut Ui, internal: &[Device], remote: bool) {
        block_title(ui, &self.i18n.t("this-computer"));
        let home = std::env::home_dir().map_or("/".into(), |h| h.to_string_lossy().into_owned());
        let mut go = ui.selectable_label(false, (icons::image(ui.ctx(), Icon::Home, TREE_ICON), home.as_str())).clicked().then_some(home.clone());
        // The folders of this computer last open, to go back there.
        let mut last: Vec<&String> = self.last_dirs.iter().filter(|(base, _)| !base.contains("://")).map(|(_, d)| d).collect();
        last.sort();
        for d in last.into_iter().filter(|d| d.trim_end_matches(['/', '\\']) != home) {
            if leaf(ui, Icon::Folder, d.as_str()).clicked() {
                go = Some(d.clone());
            }
        }
        for d in internal.iter().filter(|_| remote) {
            if leaf(ui, Icon::InternalDisk, d.name.as_str()).clicked() {
                go = Some(d.path.clone());
            }
        }
        if let Some(path) = go {
            let ctx = ui.ctx().clone();
            self.open_source(&ctx, &path);
        }
    }

    pub(crate) fn tree_blocks(&mut self, ui: &mut Ui) {
        let local = self.base == "/";
        // On Windows the internal drives go under "This computer", in drive letter order.
        let (mut internal, external): (Vec<_>, Vec<_>) = self.devices.iter().cloned().partition(|d| d.internal);
        internal.sort_by(|a, b| a.path.cmp(&b.path));
        if !external.is_empty() {
            block_title(ui, &self.i18n.t("devices"));
            // At most half of the panel, the rest goes to the computer's tree.
            let max = ui.available_height() * 0.5;
            ScrollArea::both().id_salt("devices").max_height(max).auto_shrink([false, true]).show(ui, |ui| {
                // Mounted devices are local folders: with the local source they are browsed in
                // the tree; otherwise a click opens the device as a source (also the directly
                // connected ones, `mtp://`), and from there it is browsed here.
                for (i, d) in external.iter().enumerate() {
                    let icon = if d.phone { Icon::Phone } else { Icon::Disk };
                    let label = d.name.clone();
                    let (rect, clicked) = if let Some(block) = &d.block {
                        // Not mounted: dimmed name, a click mounts it.
                        let hint = self.i18n.t_args("device-mount-hint", &[("device", block)]);
                        let r = leaf(ui, icon, egui::RichText::new(label).weak()).on_hover_text(hint);
                        (r.rect, r.clicked())
                    } else if d.path == self.base {
                        self.tree_node(ui, "device", String::new(), icon, &label, true)
                    } else if local && d.path.starts_with('/') {
                        let dir = d.path.trim_start_matches('/').to_string();
                        self.tree_node(ui, "device", dir, icon, &label, true)
                    } else {
                        let r = leaf(ui, icon, label);
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
        self.server_block(ui);
        self.tree_split = self.tree_rows.len();
        // With a device open the tree would be a duplicate: in its place, the way back home.
        // The way back is also needed if the open phone was unplugged (its error stays below).
        // With a phone or a server open, the computer stays one click away: home and the drives.
        let device_open = external.iter().any(|d| d.path == self.base);
        let remote = self.base.contains("://");
        if device_open || remote {
            self.way_home(ui, &internal, remote);
            if device_open {
                return;
            }
            ui.separator();
        }
        // The current folder is shown open in one block only: if it is inside a device,
        // the main tree does not open all the way to /run/user/…
        let in_device = local
            && self.devices.iter().any(|d| d.path.starts_with('/') && self.dir.starts_with(d.path.trim_start_matches('/')));
        let base = self.base.clone();
        if !base.contains("://") {
            block_title(ui, &self.i18n.t("this-computer"));
        }
        ScrollArea::both().id_salt("tree").auto_shrink(false).show(ui, |ui| {
            // Tree of the open source; the other internal drives open with a click.
            if !internal.iter().any(|d| d.path == base) {
                let icon = if base.starts_with("mtp://") { Icon::Phone } else { Icon::Folder };
                self.tree_node(ui, "tree", String::new(), icon, &base, !in_device);
            }
            for d in internal.iter().filter(|_| !remote) {
                if d.path == base {
                    self.tree_node(ui, "tree", String::new(), Icon::InternalDisk, &d.name, !in_device);
                } else if leaf(ui, Icon::InternalDisk, d.name.as_str()).clicked() {
                    let ctx = ui.ctx().clone();
                    self.open_source(&ctx, &d.path);
                }
            }
        });
    }

    /// `salt` tells apart the same path shown in two blocks (devices and tree).
    /// `may_reveal`: this block may open by itself down to the current folder.
    /// Returns the rectangle of the name and whether it was clicked.
    pub(crate) fn tree_node(&mut self, ui: &mut Ui, salt: &str, dir: String, icon: Icon, label: &str, may_reveal: bool) -> (Rect, bool) {
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
            // In the part receiving the keyboard: solid blue with white text; elsewhere faint.
            if self.focus == Focus::Tree {
                ui.visuals_mut().selection.bg_fill = crate::theme::solid(ui.visuals());
                ui.visuals_mut().selection.stroke.color = Color32::WHITE;
            } else {
                ui.visuals_mut().selection.bg_fill = selection_fill(ui, false);
            }
            let response = ui.selectable_label(selected, (icons::image(ui.ctx(), icon, TREE_ICON), label));
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
                    let label = self.item_name(&child);
                    self.tree_node(ui, salt, child, Icon::Folder, &label, may_reveal);
                }
            }
            // Not readable on the way to the open folder (a server's "/"): the way down still shows.
            Some(Err(_)) if on_path && !selected => {
                let child = next_on_path(&dir, &self.dir);
                let label = self.item_name(&child);
                self.tree_node(ui, salt, child, Icon::Folder, &label, may_reveal);
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
}

/// The subfolder of `dir` on the way to `open` (both ending in "/", `dir` an ancestor of `open`).
fn next_on_path(dir: &str, open: &str) -> String {
    let rest = &open[dir.len()..];
    format!("{dir}{}", rest.find('/').map_or(rest, |i| &rest[..=i]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn way_down_to_the_open_folder() {
        assert_eq!(super::next_on_path("", "home/me/Foto/"), "home/");
        assert_eq!(super::next_on_path("home/", "home/me/Foto/"), "home/me/");
        assert_eq!(super::next_on_path("home/me/", "home/me/Foto/"), "home/me/Foto/");
    }
}
