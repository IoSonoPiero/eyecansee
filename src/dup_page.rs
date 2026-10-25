//! Find duplicates: the window that asks where and shows the search, then a page of its own with
//! the groups of identical images, where the copies to delete are chosen (`duplicates`).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, ColorImage, Key, TextureHandle};

use crate::duplicates::{Group, Keep, Order, Progress};
use crate::{App, BOLD, browser, icons, source, theme};

/// Side of the thumbnail of each group.
const THUMB: f32 = 96.0;

/// Thumbnails arrived from the background, to turn into textures.
type Incoming = Arc<Mutex<Vec<(String, Result<ColorImage, String>)>>>;

/// The open search: asked (`progress` and `groups` empty), running, or its result.
pub struct Window {
    dir: String,
    sub: bool,
    progress: Option<Arc<Progress>>,
    groups: Option<Vec<Group>>,
    order: Order,
    /// Copy with the keyboard focus: group, copy.
    row: (usize, usize),
    /// The focused copy moved from the keyboard: scrolled into view.
    scroll: bool,
    /// One per group, by the path of its first copy; `None` while it comes.
    thumbs: HashMap<String, Option<Result<TextureHandle, String>>>,
    incoming: Incoming,
}

impl Window {
    /// The results page is shown (the window covers the whole interface).
    pub fn page(&self) -> bool {
        self.groups.is_some()
    }

    /// Still asking or searching: a modal, answered by Enter and Esc.
    pub fn asking(&self) -> bool {
        self.groups.is_none()
    }
}

impl App {
    /// The results page covers the window, except while one of its images is in the viewer.
    pub(crate) fn dup_page_shown(&self) -> bool {
        self.viewing.is_none() && self.duplicates.as_ref().is_some_and(Window::page)
    }

    /// Find duplicates in a folder (`None` = the one open).
    pub(crate) fn open_duplicates(&mut self, dir: Option<String>) {
        let dir = dir.filter(|d| d.ends_with('/') || d.is_empty()).unwrap_or_else(|| self.dir.clone());
        self.duplicates = Some(Window {
            dir,
            sub: true,
            progress: None,
            groups: None,
            order: Order::default(),
            row: (0, 0),
            scroll: false,
            thumbs: HashMap::new(),
            incoming: Arc::default(),
        });
    }

    fn start_duplicates(&self, w: &mut Window, ctx: &egui::Context) {
        let Some(loader) = &self.loader else { return };
        let p = Arc::new(Progress::default());
        w.progress = Some(p.clone());
        let (op, dir, sub, ctx) = (loader.store(), w.dir.clone(), w.sub, ctx.clone());
        log!("duplicates in {dir:?}, subfolders {sub}");
        self.rt.spawn(async move {
            let r = crate::duplicates::find(&op, &dir, sub, &p).await;
            *p.result.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    /// Before the results: where to look, then how the search is going. Enter starts, Esc cancels.
    pub(crate) fn duplicates_window(&mut self, ctx: &egui::Context) {
        if !self.duplicates.as_ref().is_some_and(Window::asking) {
            return;
        }
        let Some(mut w) = self.duplicates.take() else { return };
        let answer = self.delete_answer.take();
        let mut open = true;
        if let Some(p) = w.progress.clone() {
            if let Some(r) = p.result.lock().unwrap().take() {
                match r {
                    Ok(groups) => {
                        log!("duplicates: {} groups", groups.len());
                        w.groups = Some(groups);
                    }
                    Err((id, detail)) => {
                        self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                        open = false;
                    }
                }
            } else {
                open = self.searching(ctx, &p) && answer != Some(false);
                if !open {
                    p.cancel.store(true, Ordering::Relaxed);
                }
            }
        } else {
            match self.where_to_look(ctx, &mut w).or(answer) {
                Some(true) => self.start_duplicates(&mut w, ctx),
                Some(false) => open = false,
                None => {}
            }
        }
        if open {
            self.duplicates = Some(w);
        }
    }

    /// The folder and "Also in subfolders"; `Some(true)` Search, `Some(false)` Cancel.
    fn where_to_look(&self, ctx: &egui::Context, w: &mut Window) -> Option<bool> {
        let t = |id| self.i18n.t(id);
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("dup-ask")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            let folder = crate::save_dialogs::md(&self.display(&w.dir));
            crate::save_dialogs::notice_text(ui, &self.i18n.t_args("dup-ask", &[("folder", &folder)]));
            ui.add_space(6.0);
            ui.checkbox(&mut w.sub, t("dup-sub"));
            if self.base.starts_with("mtp://") || source::in_gvfs(&self.base, &w.dir) {
                ui.add_space(4.0);
                ui.label(t("dup-phone"));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(t("dup-search")).clicked() {
                    answer = Some(true);
                }
                if ui.button(t("delete-no")).clicked() {
                    answer = Some(false);
                }
            });
        });
        answer
    }

    /// The search under way: bar and counts; `false` once cancelled.
    fn searching(&self, ctx: &egui::Context, p: &Progress) -> bool {
        let (found, examined, read) = (p.found.load(Ordering::Relaxed), p.examined.load(Ordering::Relaxed), p.read.load(Ordering::Relaxed));
        let mut open = true;
        egui::Modal::new(egui::Id::unique("dup-progress")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(self.i18n.t("dup-searching"));
            ui.add_space(6.0);
            ui.add(egui::ProgressBar::new(examined as f32 / found.max(1) as f32));
            let (n, m, size) = (examined.to_string(), found.to_string(), self.human_size(read));
            ui.label(self.i18n.t_args("dup-progress", &[("n", &n), ("m", &m), ("size", &size)]));
            ui.add_space(8.0);
            if ui.button(self.i18n.t("delete-no")).clicked() {
                open = false;
            }
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        open
    }

    /// A copy deleted (from the page or anywhere else): out of its group.
    pub(crate) fn dup_deleted(&mut self, path: &str) {
        let Some(groups) = self.duplicates.as_mut().and_then(|w| w.groups.as_mut()) else { return };
        groups.retain_mut(|g| g.remove(path));
        let w = self.duplicates.as_mut().unwrap();
        w.row = clamp(w.groups.as_deref().unwrap_or_default(), w.row);
    }

    /// An image in the viewer: its folder opens behind the page, which comes back with Esc.
    fn dup_view(&mut self, path: String) {
        self.set_dir(source::parent(&path));
        match self.listings.get(&self.dir) {
            Some(Ok(l)) => {
                if let Some(i) = l.files.iter().position(|f| f.path == path) {
                    self.selected = l.dirs.len() + i;
                    self.cursor = true;
                    self.viewing = Some(i);
                }
            }
            _ => self.start = Some(path),
        }
    }

    /// The results, in place of the whole interface like "Save in another format".
    pub(crate) fn dup_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(mut w) = self.duplicates.take() else { return };
        for (path, image) in w.incoming.lock().unwrap().drain(..) {
            let tex = image.map(|i| ctx.load_texture(format!("dup-{path}"), i, egui::TextureOptions::LINEAR));
            w.thumbs.insert(path, Some(tex));
        }
        let (mut close, mut delete) = (dup_keys(&ctx, &mut w), false);
        egui::Panel::top("dup-head").show(ui, |ui| close |= !self.dup_header(ui, &mut w));
        egui::Panel::bottom("dup-foot").show(ui, |ui| {
            let (c, d) = self.dup_footer(ui, &w);
            (close, delete) = (close | c, delete | d);
        });
        let mut view = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).fill(theme::canvas(ui.visuals())))
            .show(ui, |ui| view = self.dup_groups(ui, &mut w));
        if delete {
            let marked: Vec<&source::File> = w.groups.iter().flatten().flat_map(|g| g.files.iter().zip(&g.delete)).filter(|(_, d)| **d).map(|(f, _)| f).collect();
            if !marked.is_empty() && self.deleting == 0 {
                let size = marked.iter().map(|f| f.size).sum();
                self.ask_delete_paths(marked.iter().map(|f| f.path.clone()).collect(), false, Some(size));
            }
        }
        if !close {
            self.duplicates = Some(w);
        }
        if let Some(path) = view {
            self.dup_view(path);
        }
    }

    /// Title, order and the buttons that mark every group at once; `false` once closed.
    fn dup_header(&self, ui: &mut egui::Ui, w: &mut Window) -> bool {
        let t = |id| self.i18n.t(id);
        let mut open = true;
        ui.horizontal(|ui| {
            ui.heading(self.i18n.t_args("dup-title", &[("folder", &self.display(&w.dir))]));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕").on_hover_text("Esc").clicked() {
                    open = false;
                }
            });
        });
        let Some(groups) = &mut w.groups else { return open };
        if groups.is_empty() {
            return open;
        }
        ui.horizontal(|ui| {
            ui.label(t("dup-order"));
            let before = w.order;
            egui::ComboBox::from_id_salt("dup-order")
                .selected_text(t(order_label(w.order)))
                .show_ui(ui, |ui| {
                    for o in [Order::Date, Order::Name, Order::Path] {
                        ui.selectable_value(&mut w.order, o, t(order_label(o)));
                    }
                });
            if w.order != before {
                groups.iter_mut().for_each(|g| g.sort(w.order));
            }
            ui.separator();
            for (keep, id) in [(Keep::First, "dup-keep-first"), (Keep::Last, "dup-keep-last"), (Keep::All, "dup-keep-all")] {
                if ui.button(t(id)).clicked() {
                    groups.iter_mut().for_each(|g| g.keep(keep));
                }
            }
        });
        ui.add_space(2.0);
        open
    }

    /// What deleting will do, Close and Delete: (close, delete) clicked.
    fn dup_footer(&self, ui: &mut egui::Ui, w: &Window) -> (bool, bool) {
        let groups = w.groups.as_deref().unwrap_or_default();
        let marked = groups.iter().flat_map(|g| g.files.iter().zip(&g.delete)).filter(|(_, d)| **d);
        let (n, size) = marked.fold((0, 0), |(n, s), (f, _)| (n + 1, s + f.size));
        let (mut close, mut delete) = (false, false);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if !groups.is_empty() {
                let args = [("groups", groups.len().to_string()), ("n", n.to_string()), ("size", self.human_size(size))];
                ui.label(self.i18n.t_args("dup-summary", &args.each_ref().map(|(k, v)| (*k, v.as_str()))));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = egui::RichText::new(self.i18n.t("dup-delete")).family(egui::FontFamily::Name(BOLD.into()));
                delete = ui.add_enabled(n > 0 && self.deleting == 0, egui::Button::new(label)).clicked();
                close = ui.button(self.i18n.t("notice-close")).on_hover_text("Esc").clicked();
            });
        });
        ui.add_space(2.0);
        (close, delete)
    }

    /// Every group in a frame of its own; the image clicked to open in the viewer.
    fn dup_groups(&self, ui: &mut egui::Ui, w: &mut Window) -> Option<String> {
        let groups = w.groups.take().unwrap_or_default();
        let mut view = None;
        if groups.is_empty() {
            ui.centered_and_justified(|ui| ui.label(self.i18n.t("dup-none")));
        }
        let mut groups = groups;
        // ponytail: every group laid out at every frame; show_rows-style virtualising if thousands make it slow
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (gi, g) in groups.iter_mut().enumerate() {
                if let Some(path) = self.dup_group(ui, w, gi, g) {
                    view = Some(path);
                }
                ui.add_space(8.0);
            }
        });
        w.groups = Some(groups);
        w.scroll = false;
        view
    }

    fn dup_group(&self, ui: &mut egui::Ui, w: &mut Window, gi: usize, g: &mut Group) -> Option<String> {
        let (fill, stroke) = theme::tile(ui.visuals());
        let mut view = None;
        egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, stroke)).corner_radius(6).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                if self.dup_thumb(ui, w, &g.files[0]) {
                    view = Some(g.files[0].path.clone());
                }
                ui.vertical(|ui| {
                    let (n, size) = (g.files.len().to_string(), self.human_size(g.files[0].size));
                    let title = self.i18n.t_args("dup-group", &[("n", &n), ("size", &size)]);
                    ui.label(egui::RichText::new(title).family(egui::FontFamily::Name(BOLD.into())));
                    ui.add_space(2.0);
                    for i in 0..g.files.len() {
                        self.dup_copy(ui, w, g, (gi, i));
                    }
                });
            });
        });
        view
    }

    /// The group's thumbnail, asked for the first time it is on screen; `true` when clicked.
    fn dup_thumb(&self, ui: &mut egui::Ui, w: &mut Window, file: &source::File) -> bool {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(THUMB, THUMB), egui::Sense::click());
        if ui.is_rect_visible(rect)
            && !w.thumbs.contains_key(&file.path)
            && let Some(loader) = &self.loader
        {
            w.thumbs.insert(file.path.clone(), None);
            let (op, base, file, incoming, ctx) = (loader.store(), self.base.clone(), file.clone(), w.incoming.clone(), ui.ctx().clone());
            self.rt.spawn(async move {
                let image = crate::loader::thumbnail(&op, &base, file.clone()).await;
                incoming.lock().unwrap().push((file.path, image));
                ctx.request_repaint();
            });
        }
        match w.thumbs.get(&file.path) {
            Some(Some(Ok(tex))) => {
                let fit = egui::Rect::from_center_size(rect.center(), tex.size_vec2() * (THUMB / tex.size_vec2().max_elem()));
                egui::Image::new(tex).paint_at(ui, fit);
            }
            Some(Some(Err(e))) => {
                ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, theme::tile(ui.visuals()).1), egui::StrokeKind::Inside);
                return response.on_hover_text(e.as_str()).clicked();
            }
            _ => {
                egui::Spinner::new().paint_at(ui, rect.shrink(THUMB / 3.0));
            }
        }
        response.on_hover_text(self.i18n.t("dup-view")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    /// One copy: the box that marks it (the text too), whether it stays, date and path. The copy to delete is
    /// faded and struck through, the one that stays at full strength with "Stays".
    fn dup_copy(&self, ui: &mut egui::Ui, w: &mut Window, g: &mut Group, at: (usize, usize)) {
        let i = at.1;
        let focused = w.row == at;
        let fill = if focused { browser::selection_fill(ui, true) } else { egui::Color32::TRANSPARENT };
        let row = egui::Frame::new().fill(fill).corner_radius(4).inner_margin(egui::Margin::symmetric(4, 1)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut marked = g.delete[i];
                let r = ui.add_enabled(g.can_toggle(i), egui::Checkbox::without_text(&mut marked));
                if r.on_hover_text(self.i18n.t("dup-mark")).on_disabled_hover_text(self.i18n.t("dup-last-one")).changed() {
                    g.toggle(i);
                    w.row = at;
                }
                let file = &g.files[i];
                let date = self.file_date(&file.modified).unwrap_or_default();
                // Below the folder searched, which the title shows: the full path on hover.
                let text = format!("{date}    {}", file.path.strip_prefix(&w.dir).unwrap_or(&file.path));
                let text = if g.delete[i] {
                    ui.add(icons::image(ui.ctx(), icons::Icon::Trash, 16.0));
                    egui::RichText::new(text).weak().strikethrough()
                } else {
                    ui.label(egui::RichText::new(self.i18n.t("dup-stays")).color(theme::accent(ui.visuals())).family(egui::FontFamily::Name(BOLD.into())));
                    egui::RichText::new(text)
                };
                // The text too marks and unmarks, like the box: a small target is easy to miss.
                let label = ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(self.display(&file.path));
                if label.clicked() {
                    g.toggle(i);
                    w.row = at;
                }
            });
        });
        if focused && w.scroll {
            row.response.scroll_to_me(None);
        }
    }
}

fn order_label(o: Order) -> &'static str {
    match o {
        Order::Date => "dup-by-date",
        Order::Name => "dup-by-name",
        Order::Path => "dup-by-path",
    }
}

/// The focus on a copy that exists (after deletions).
fn clamp(groups: &[Group], (g, i): (usize, usize)) -> (usize, usize) {
    match groups.get(g) {
        Some(group) => (g, i.min(group.files.len() - 1)),
        None if groups.is_empty() => (0, 0),
        None => (groups.len() - 1, 0),
    }
}

/// Arrows move between the copies of every group, Space marks, Esc closes (`true`). Enter does
/// not delete: in a list it reads as "choose", only the button deletes.
fn dup_keys(ctx: &egui::Context, w: &mut Window) -> bool {
    if ctx.egui_wants_keyboard_input() {
        return false;
    }
    let (up, down, space, esc) = ctx.input(|i| {
        (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Space), i.key_pressed(Key::Escape))
    });
    let Some(groups) = w.groups.as_mut().filter(|g| !g.is_empty()) else { return esc };
    let (g, i) = clamp(groups, w.row);
    w.row = match () {
        _ if down && i + 1 < groups[g].files.len() => (g, i + 1),
        _ if down && g + 1 < groups.len() => (g + 1, 0),
        _ if up && i > 0 => (g, i - 1),
        _ if up && g > 0 => (g - 1, groups[g - 1].files.len() - 1),
        _ => (g, i),
    };
    w.scroll |= up || down;
    if space {
        groups[w.row.0].toggle(w.row.1);
    }
    esc
}
