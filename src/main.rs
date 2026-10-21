// In release on Windows, no console window next to the program's window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Diagnostic log: with `EYECANSEE_LOG=1` writes to stderr what happens, with the seconds since
/// startup. Defined before the modules so they can use it too.
macro_rules! log {
    ($($arg:tt)*) => {
        if $crate::log_enabled() {
            eprintln!("[{:8.3}] {}", $crate::log_time(), format!($($arg)*));
        }
    };
}

fn log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("EYECANSEE_LOG").is_some())
}

fn log_time() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

mod browser;
mod decode;
mod devices;
mod i18n;
mod loader;
mod meta;
mod mtp;
mod settings;
mod source;
mod theme;
mod thumbcache;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Key, TextureHandle, TextureOptions};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use i18n::I18n;
use loader::{Loader, Msg, THUMB_SIDE};
use settings::{Settings, ThemeChoice, WindowStart};
use source::{File, Listing};

/// Version number: it lives only in the `VERSION` file (and, identical, in `Cargo.toml`; a test checks it).
fn version() -> &'static str {
    include_str!("../VERSION").trim()
}

/// Drag of the grid's scroll bar; `lost` when the pointer has left the window and egui has
/// stopped following it.
pub(crate) struct BarDrag {
    /// Distance of the mouse from the top edge of the handle.
    grab: f32,
    lost: bool,
}

/// Part of the window that receives arrows and Tab.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Focus {
    Grid,
    /// Device highlighted in the Devices block (Enter opens it).
    Devices(usize),
    Tree,
}

#[derive(Clone, Copy, Debug)]
enum Go {
    Next,
    Prev,
    First,
    Last,
}

/// Images kept in memory around the one open in the viewer, on each side.
const AROUND: usize = 2;
/// Seconds between attempts on a phone that shows no storage (like the device check).
const RETRY_SECS: f64 = 2.0;

struct App {
    i18n: I18n,
    /// Available languages (code, name in the language itself), for the Options.
    languages: Vec<(&'static str, String)>,
    rt: tokio::runtime::Runtime,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    loader: Option<Loader>,
    /// Source prefix shown in the address bar.
    base: String,
    /// Current folder, relative to the source's root.
    dir: String,
    /// Listings already read, used by both the tree and the grid. F5 empties them.
    listings: HashMap<String, Result<Listing, String>>,
    listing_pending: HashSet<String>,
    generation: u64,
    /// Item selected in the grid: folders first, then images.
    selected: usize,
    /// Selection shown. On entering a folder there is none: it appears at the first click or key
    /// (arrows from the first item), otherwise it looked like another folder chosen next to the tree.
    cursor: bool,
    /// Part receiving arrows and Tab: the last click decides, then Tab moves from one to the other.
    focus: Focus,
    /// Tree nodes in the order they appear (block, folder), for the arrows; from `tree_split`
    /// onwards those of "This computer", before them those of the devices.
    tree_rows: Vec<(egui::Id, String)>,
    tree_split: usize,
    /// Folder just opened: the grid starts again from the first preview (unless `remember_scroll`).
    scroll_top: bool,
    /// Metadata of the selected image (index among the images) and last request made.
    meta: Option<(usize, Result<Option<meta::Meta>, String>)>,
    meta_asked: Option<(u64, usize)>,
    /// Height of the metadata panel with the data shown, kept while the new data loads.
    meta_height: f32,
    /// Phone without storage in the open folder: when to read it again (egui time).
    retry_at: Option<f64>,
    /// The framed device was changed from the keyboard: it must be scrolled into view.
    scroll_to_device: bool,
    /// Drag of the grid's scroll bar in progress.
    bar_drag: Option<BarDrag>,
    /// Area of the grid's cells in the previous frame (the bar is on its right).
    grid_rect: Option<egui::Rect>,
    /// Tab pressed (`Some(true)` with Shift: reverse direction), removed from the input before
    /// egui uses it to move between its controls.
    tab: Option<bool>,
    scroll_to_selected: bool,
    /// Index (among the images) of the one open in the viewer.
    viewing: Option<usize>,
    /// Image to open as soon as the folder's listing arrives.
    start: Option<String>,
    /// Thumbnails in memory: only those in the window around the visible rows (`thumb_keep`).
    thumbs: HashMap<usize, Result<TextureHandle, String>>,
    thumb_keep: std::ops::Range<usize>,
    /// Thumbnails already done (on disk), not to be queued again for background generation.
    generated: HashSet<usize>,
    /// Last queue sent to the loader: (visible images, selected, generation).
    thumb_queue_key: Option<(std::ops::Range<usize>, Option<usize>, u64)>,
    full: HashMap<usize, Result<TextureHandle, String>>,
    /// Viewer images already requested from the loader.
    pending: HashSet<usize>,
    thumb_size: f32,
    address: String,
    error: Option<String>,
    title: String,
    help: bool,
    help_cache: CommonMarkCache,
    settings: Settings,
    devices: Vec<devices::Device>,
    /// The tree must open down to the current folder (after every folder change).
    reveal: bool,
    /// Options window open.
    options: bool,
    /// Confirmation of emptying the cache in progress: (thumbnails, bytes).
    purge_confirm: Option<(u64, u64)>,
    /// Sources already gone through cache maintenance in this session.
    maintained: HashSet<String>,
}

impl App {
    /// `arg`: address from the command line. Without it (e.g. started from the applications menu)
    /// the last folder viewed if the option is on, otherwise the home folder.
    fn new(ctx: &egui::Context, settings: Settings, arg: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            i18n: I18n::detect(settings.language.as_deref()),
            languages: i18n::languages(),
            rt: tokio::runtime::Runtime::new().expect("tokio runtime"),
            tx,
            rx,
            loader: None,
            base: String::new(),
            dir: String::new(),
            listings: HashMap::new(),
            listing_pending: HashSet::new(),
            generation: 0,
            selected: 0,
            cursor: false,
            focus: Focus::Grid,
            tree_rows: Vec::new(),
            tree_split: 0,
            scroll_top: false,
            meta: None,
            meta_asked: None,
            meta_height: 0.0,
            retry_at: None,
            scroll_to_device: false,
            bar_drag: None,
            grid_rect: None,
            tab: None,
            scroll_to_selected: false,
            viewing: None,
            start: None,
            thumbs: HashMap::new(),
            thumb_keep: 0..0,
            generated: HashSet::new(),
            thumb_queue_key: None,
            full: HashMap::new(),
            pending: HashSet::new(),
            thumb_size: 160.0,
            address: String::new(),
            error: None,
            title: String::new(),
            help: false,
            help_cache: CommonMarkCache::default(),
            settings,
            reveal: true,
            // Straight away, not only from the thread: the fallback already needs it if the first address is a busy mtp://.
            devices: devices::scan(),
            options: false,
            purge_confirm: None,
            maintained: HashSet::new(),
        };
        theme::apply(ctx, app.settings.theme);
        devices::watch(app.tx.clone(), ctx.clone());
        let home = std::env::home_dir().map_or(".".into(), |h| h.to_string_lossy().into_owned());
        let last = app.settings.last_location.clone().filter(|_| app.settings.remember_location);
        match (arg, last) {
            (Some(arg), _) => app.open_source(ctx, &arg),
            (None, Some(last)) => {
                // Folder gone, disk or phone unplugged: start from home, without an error.
                // An mtp:// would open anyway (the phone connects at the first listing, whose
                // error would arrive later, in the home tree): `devices` tells.
                // By serial: the address can be a subfolder, and on GNOME the same phone shows up
                // as a gvfs mount (then `mounted_twin` switches to it by itself).
                let serial = last.strip_prefix("mtp://").map(|s| s.split('/').next().unwrap_or_default());
                let phone_gone = serial.is_some_and(|s| !app.devices.iter().any(|d| d.serial.as_deref() == Some(s)));
                if !phone_gone {
                    app.open_source(ctx, &last);
                }
                if app.loader.is_none() || phone_gone {
                    app.open_source(ctx, &home);
                }
            }
            (None, None) => app.open_source(ctx, &home),
        }
        app
    }

    /// Opens an address (path or URL). If it fails, stays on the current source.
    fn open_source(&mut self, ctx: &egui::Context, address: &str) {
        log!("open source {address:?}");
        let opened = {
            let _guard = self.rt.enter();
            source::open(address.trim())
        };
        match opened {
            Ok(s) => {
                let handle = self.rt.handle().clone();
                self.loader = Some(Loader::new(handle, s.op, s.base.clone(), self.tx.clone(), ctx.clone()));
                self.base = s.base;
                self.listings.clear();
                self.listing_pending.clear();
                self.error = None;
                self.set_dir(s.dir);
                self.start = s.start;
                self.maintain();
            }
            Err((id, detail)) => {
                log!("open failed: {id} {detail}");
                self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                self.address = self.display(&self.dir);
            }
        }
    }

    fn maintain(&mut self) {
        if self.settings.thumb_maintenance
            && let Some(loader) = &self.loader
            && self.maintained.insert(self.base.clone())
        {
            loader.maintain(self.settings.thumb_cache_mb as u64 * 1024 * 1024);
        }
    }

    /// Every change is saved immediately.
    fn options_window(&mut self, ctx: &egui::Context) {
        let mut open = self.options;
        let before = self.settings.clone();
        let t = |id| self.i18n.t(id);
        let (window, normal, maximized, remember_size) =
            (t("opt-window"), t("opt-window-normal"), t("opt-window-maximized"), t("opt-window-remember"));
        let (scroll, scroll_hint) = (t("opt-remember-scroll"), t("opt-remember-scroll-hint"));
        let (language_label, language_system) = (t("opt-language"), t("opt-language-system"));
        let languages = self.languages.clone();
        let (theme_label, theme_system, theme_light, theme_dark) =
            (t("opt-theme"), t("opt-theme-system"), t("opt-theme-light"), t("opt-theme-dark"));
        let (title, remember, remember_hint, maint, maint_hint, limit) = (
            t("options-title"),
            t("opt-remember-location"),
            t("opt-remember-location-hint"),
            t("opt-cache-maintenance"),
            t("opt-cache-maintenance-hint"),
            t("opt-cache-limit"),
        );
        let (purge, purge_no) = (t("opt-purge"), t("opt-purge-no"));
        let purge_text = self.purge_confirm.map(|(n, bytes)| {
            let mb = format!("{:.1}", bytes as f64 / 1_048_576.0);
            (self.i18n.t_args("opt-purge-confirm", &[("n", &n.to_string()), ("mb", &mb)]), self.i18n.t("opt-purge-yes"))
        });
        let mut purged = false;
        let confirm = &mut self.purge_confirm;
        let s = &mut self.settings;
        // Fixed id: changing language changes the title, and the window must not move.
        egui::Window::new(title).id(egui::Id::unique("options")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.checkbox(&mut s.remember_location, remember).on_hover_text(remember_hint);
            ui.checkbox(&mut s.remember_scroll, scroll).on_hover_text(scroll_hint);
            ui.separator();
            ui.label(window);
            ui.radio_value(&mut s.window_start, WindowStart::Normal, normal);
            ui.radio_value(&mut s.window_start, WindowStart::Maximized, maximized);
            ui.radio_value(&mut s.window_start, WindowStart::Remember, remember_size);
            ui.separator();
            ui.label(language_label);
            ui.radio_value(&mut s.language, None, language_system);
            for (code, name) in languages {
                ui.radio_value(&mut s.language, Some(code.to_string()), name);
            }
            ui.separator();
            ui.label(theme_label);
            ui.radio_value(&mut s.theme, ThemeChoice::System, theme_system);
            ui.radio_value(&mut s.theme, ThemeChoice::Light, theme_light);
            ui.radio_value(&mut s.theme, ThemeChoice::Dark, theme_dark);
            ui.separator();
            ui.checkbox(&mut s.thumb_maintenance, maint).on_hover_text(maint_hint);
            ui.add_enabled_ui(s.thumb_maintenance, |ui| {
                ui.horizontal(|ui| {
                    ui.label(limit);
                    ui.add(egui::DragValue::new(&mut s.thumb_cache_mb).range(16..=1_000_000).suffix(" MB"));
                });
            });
            ui.separator();
            match &purge_text {
                None => {
                    if ui.button(&purge).clicked() {
                        *confirm = Some(thumbcache::stats());
                    }
                }
                Some((text, yes)) => {
                    ui.label(text);
                    ui.horizontal(|ui| {
                        if ui.button(yes).clicked() {
                            thumbcache::purge();
                            purged = true;
                            *confirm = None;
                        }
                        if ui.button(&purge_no).clicked() {
                            *confirm = None;
                        }
                    });
                }
            }
        });
        if !open {
            self.purge_confirm = None;
        }
        if purged {
            log!("thumbnail cache emptied");
            // The thumbnails in memory are still the old ones: the open folder redoes them right away.
            self.refresh();
        }
        self.options = open;
        if self.settings.language != before.language {
            log!("language {:?}", self.settings.language);
            self.i18n = I18n::detect(self.settings.language.as_deref());
            // The cached manual is the one of the previous language.
            self.help_cache = CommonMarkCache::default();
        }
        if self.settings.theme != before.theme {
            theme::choose(ctx, self.settings.theme);
        }
        if self.settings != before {
            self.settings.save();
            self.maintain();
        }
    }

    /// For the `mtp://<serial>/` source, the desktop's mount of the same phone.
    fn mounted_twin(&self) -> Option<String> {
        devices::gvfs_mount(self.base.strip_prefix("mtp://")?.trim_end_matches('/'))
    }

    fn display(&self, dir: &str) -> String {
        let sep = if self.base.contains("://") || !self.base.contains('\\') { "/" } else { "\\" };
        format!("{}{}", self.base, dir.replace('/', sep))
    }

    pub(crate) fn set_dir(&mut self, dir: String) {
        log!("folder {dir:?}");
        self.reveal = true;
        if let Some(loader) = &self.loader {
            self.generation = loader.next_generation();
        }
        self.address = self.display(&dir);
        self.dir = dir;
        self.thumbs.clear();
        self.generated.clear();
        self.thumb_queue_key = None;
        self.full.clear();
        self.pending.clear();
        self.selected = 0;
        self.cursor = false;
        self.scroll_top = true;
        self.meta = None;
        self.meta_asked = None;
        self.retry_at = None;
        self.viewing = None;
        self.start = None;
        let dir = self.dir.clone();
        self.request_listing(&dir);
    }

    pub(crate) fn go_up(&mut self) {
        if !self.dir.is_empty() {
            let child = self.dir.clone();
            self.set_dir(source::parent(&child));
            // The folder we came up from is selected.
            if let Some(Ok(l)) = self.listings.get(&self.dir)
                && let Some(i) = l.dirs.iter().position(|d| *d == child)
            {
                self.selected = i;
                self.cursor = true;
                self.scroll_to_selected = true;
            }
        }
    }

    /// Reads the folders again. The viewer keeps the same image open, if it still exists: until
    /// the new listing arrives the old one is used, so the grid does not flash for a moment.
    fn refresh(&mut self) {
        log!("refresh");
        let viewing = self.viewing;
        let path = viewing.and_then(|i| self.files().get(i)).map(|f| f.path.clone());
        let old = self.listings.remove(&self.dir);
        self.listings.clear();
        let dir = self.dir.clone();
        self.set_dir(dir);
        if let (Some(i), Some(path), Some(old)) = (viewing, path, old) {
            self.listings.insert(self.dir.clone(), old);
            self.viewing = Some(i);
            self.start = Some(path);
        }
    }

    pub(crate) fn request_listing(&mut self, dir: &str) {
        if let Some(loader) = &self.loader
            && !self.listings.contains_key(dir)
            && self.listing_pending.insert(dir.to_string())
        {
            loader.list(dir.to_string());
        }
    }

    fn files(&self) -> &[File] {
        match self.listings.get(&self.dir) {
            Some(Ok(l)) => &l.files,
            _ => &[],
        }
    }

    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Listed(dir, r) => {
                    match &r {
                        Ok(l) => log!("listing {dir:?}: {} folders, {} images", l.dirs.len(), l.files.len()),
                        Err(e) => log!("listing {dir:?}: ERROR {e:?}"),
                    }
                    self.listing_pending.remove(&dir);
                    let mut r = r;
                    if let Ok(l) = &mut r {
                        l.sort_files(self.settings.sort_by, self.settings.sort_desc);
                    }
                    // Direct connection impossible because the desktop holds the phone: switch to
                    // its mount, in the same folder (the storages have the same name).
                    if let Err(("error-device-busy", _)) = &r
                        && let Some(mounted) = self.mounted_twin()
                    {
                        self.open_source(ctx, &format!("{mounted}{dir}"));
                        continue;
                    }
                    if dir == self.dir
                        && let Some(start) = self.start.take()
                    {
                        let found = r.as_ref().ok().and_then(|l| {
                            l.files.iter().position(|f| f.path == start).map(|i| (l.dirs.len(), i))
                        });
                        // After Refresh the image may have changed position or be gone:
                        // images already loaded by position are no longer valid.
                        if self.viewing.is_some() && self.viewing != found.map(|(_, i)| i) {
                            if let Some(loader) = &self.loader {
                                self.generation = loader.next_generation();
                            }
                            self.full.clear();
                            self.pending.clear();
                            self.thumbs.clear();
                            self.generated.clear();
                            self.thumb_queue_key = None;
                        }
                        self.viewing = found.map(|(_, i)| i);
                        if let Some((ndirs, i)) = found {
                            self.selected = ndirs + i;
                            self.cursor = true;
                            self.scroll_to_selected = true;
                        }
                    }
                    if dir == self.dir {
                        let no_storage = match &r {
                            Err((id, _)) => *id == "error-no-storage" || *id == "error-no-device",
                            Ok(l) => l.dirs.is_empty() && l.files.is_empty() && self.phone_root(&dir).is_some(),
                        };
                        self.retry_at = no_storage.then(|| ctx.input(|i| i.time) + RETRY_SECS);
                    }
                    let r = r.map_err(|(id, detail)| self.i18n.t_args(id, &[("detail", &detail)]));
                    self.listings.insert(dir, r);
                }
                Msg::Loaded { generation, index, thumb, image } if generation == self.generation => {
                    let name = self.files().get(index).map(|f| f.path.clone()).unwrap_or_default();
                    match &image {
                        Err(e) => log!("{} {index} {name:?}: ERROR {e}", if thumb { "thumbnail" } else { "image" }),
                        Ok(img) if !thumb => log!("image {index} {name:?}: {}x{}", img.size[0], img.size[1]),
                        Ok(_) => {}
                    }
                    if thumb {
                        self.generated.insert(index);
                        // Generated in the background far from the screen: it stays on disk only.
                        if self.thumb_keep.contains(&index) {
                            let tex = image.map(|img| ctx.load_texture(name, img, TextureOptions::LINEAR));
                            self.thumbs.insert(index, tex);
                        }
                    } else {
                        self.pending.remove(&index);
                        let tex = image.map(|img| ctx.load_texture(name, img, TextureOptions::LINEAR));
                        self.full.insert(index, tex);
                    }
                }
                Msg::Meta { generation, index, meta } if generation == self.generation => {
                    if let Err(e) = &meta {
                        log!("metadata {index}: ERROR {e}");
                    }
                    self.meta = Some((index, meta));
                }
                Msg::Loaded { .. } | Msg::Meta { .. } => {} // folder has changed in the meantime
                Msg::Mounted(Ok(path)) => {
                    log!("mounted at {path:?}");
                    // Opened straight away: as a local folder, or reopening the disk if the source is remote.
                    if self.base == "/" {
                        self.set_dir(format!("{}/", path.trim_matches('/')));
                    } else {
                        self.open_source(ctx, &path);
                    }
                }
                Msg::Mounted(Err(e)) => {
                    log!("mount failed: {e}");
                    self.error = Some(self.i18n.t_args("error-mount", &[("detail", &e)]));
                }
                Msg::Devices(d) => {
                    log!("devices: {:?}", d.iter().map(|d| &d.path).collect::<Vec<_>>());
                    self.devices = d;
                }
            }
        }
    }

    /// Phone without storage (locked, charging or not yet in "File transfer") or not found (it
    /// reconnects when switched to "File transfer", or was unplugged): the folder is
    /// read again by itself every `RETRY_SECS`, without F5. Until then the message stays.
    fn retry(&mut self, ctx: &egui::Context) {
        let Some(at) = self.retry_at else { return };
        let now = ctx.input(|i| i.time);
        if now < at {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            return;
        }
        self.retry_at = None;
        if let Some(loader) = &self.loader
            && self.listing_pending.insert(self.dir.clone())
        {
            log!("retry {:?}", self.dir);
            loader.list(self.dir.clone());
        }
    }

    fn global_keys(&mut self, ctx: &egui::Context) {
        let (help, refresh) = ctx.input(|i| (i.key_pressed(Key::F1), i.key_pressed(Key::F5)));
        if help {
            self.help = !self.help;
        }
        if refresh {
            self.refresh();
        }
    }

    fn viewer_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (next, prev, first, last, close) = ctx.input(|i| {
            let any = |keys: &[Key]| keys.iter().any(|k| i.key_pressed(*k));
            (
                any(&[Key::ArrowRight, Key::Space, Key::PageDown]),
                any(&[Key::ArrowLeft, Key::Backspace, Key::PageUp]),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::Escape),
            )
        });
        for (pressed, to) in [(next, Go::Next), (prev, Go::Prev), (first, Go::First), (last, Go::Last)] {
            if pressed {
                self.go(to);
            }
        }
        if close {
            self.close_viewer();
        }
    }

    /// Next, previous, first, last: same action from the keyboard and from the Go menu.
    /// In the viewer it changes image (after the last one it goes back to the first), in the grid it moves the selection.
    pub(crate) fn go(&mut self, to: Go) {
        if let Some(current) = self.viewing {
            let n = self.files().len();
            if n > 0 {
                self.viewing = Some(match to {
                    Go::Next => (current + 1) % n,
                    Go::Prev => (current + n - 1) % n,
                    Go::First => 0,
                    Go::Last => n - 1,
                });
            }
        } else {
            let n = self.items();
            if n > 0 {
                // With no selection shown, next and previous start from the first item.
                self.selected = match to {
                    Go::Next if self.cursor => (self.selected + 1).min(n - 1),
                    Go::Prev => self.selected.saturating_sub(1),
                    Go::First | Go::Next => 0,
                    Go::Last => n - 1,
                };
                self.cursor = true;
                self.scroll_to_selected = true;
            }
        }
    }

    /// Grid items: folders plus images.
    fn items(&self) -> usize {
        match self.listings.get(&self.dir) {
            Some(Ok(l)) => l.dirs.len() + l.files.len(),
            _ => 0,
        }
    }

    fn close_viewer(&mut self) {
        if let (Some(i), Some(Ok(l))) = (self.viewing.take(), self.listings.get(&self.dir)) {
            self.selected = l.dirs.len() + i;
            self.cursor = true;
            self.scroll_to_selected = true;
        }
        self.full.clear();
    }

    /// Requests the open image and its neighbours, frees the distant ones.
    fn prefetch_full(&mut self, current: usize) {
        let n = self.files().len();
        let (Some(loader), Some(Ok(listing))) = (&self.loader, self.listings.get(&self.dir)) else { return };
        if n == 0 {
            return;
        }
        let order = (0..=AROUND).flat_map(|d| [current + d, current + n * AROUND - d]);
        for i in order.map(|i| i % n) {
            if !self.full.contains_key(&i) && self.pending.insert(i) {
                loader.load_full(self.generation, i, listing.files[i].path.clone());
            }
        }
        let distance = |i: usize| i.abs_diff(current).min(n - i.abs_diff(current));
        self.full.retain(|&i, _| distance(i) <= AROUND + 1);
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match self.viewing {
            Some(i) => {
                let n = self.files().len();
                let counter = self.i18n.t_args("counter", &[("n", &(i + 1).to_string()), ("total", &n.to_string())]);
                let name = self.files().get(i).map_or("", |f| source::name(&f.path));
                format!("{name} ({counter}) - EyeCanSee")
            }
            None => format!("{} - EyeCanSee", self.display(&self.dir)),
        };
        if title != self.title {
            log!("title {title:?}");
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Every keyboard or double-click action also has a menu item, with the shortcut next to it.
    /// The menus only record the chosen item; the action runs afterwards, with access to the whole app.
    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        #[derive(Debug)]
        enum Act {
            Address,
            Options,
            Quit,
            Open,
            Back,
            Refresh,
            Go(Go),
            Up,
            Manual,
            Meta(meta::Group),
            Sort(settings::SortBy),
            SortDesc,
        }
        let t = |id| self.i18n.t(id);
        let item = |label: String, keys: String| egui::Button::new(label).shortcut_text(keys);
        let viewing = self.viewing.is_some();
        let has_items = self.items() > 0;
        let can_go_up = !viewing && !self.dir.is_empty();
        let mut act = None;
        egui::MenuBar::new().ui(ui, |ui| {
            let mut entry = |ui: &mut egui::Ui, enabled: bool, button: egui::Button, a: Act| {
                if ui.add_enabled(enabled, button).clicked() {
                    act = Some(a);
                }
            };
            ui.menu_button(t("menu-file"), |ui| {
                entry(ui, true, egui::Button::new(t("menu-open-location")), Act::Address);
                entry(ui, true, egui::Button::new(t("menu-options")), Act::Options);
                ui.separator();
                entry(ui, true, egui::Button::new(t("menu-quit")), Act::Quit);
            });
            ui.menu_button(t("menu-view"), |ui| {
                entry(ui, !viewing && has_items && self.cursor, item(t("menu-open-selected"), t("key-enter")), Act::Open);
                entry(ui, viewing, item(t("menu-back-to-grid"), "Esc".into()), Act::Back);
                ui.separator();
                entry(ui, true, item(t("menu-refresh"), "F5".into()), Act::Refresh);
                ui.separator();
                ui.weak(t("sort-by"));
                for by in settings::SortBy::ALL {
                    let current = self.settings.sort_by == by;
                    entry(ui, true, egui::Button::selectable(current, t(by.label())), Act::Sort(by));
                }
                entry(ui, true, egui::Button::selectable(self.settings.sort_desc, t("sort-desc")), Act::SortDesc);
                ui.separator();
                for g in meta::Group::ALL {
                    let label = self.i18n.t(&format!("meta-{}", g.key()));
                    entry(ui, !viewing, egui::Button::selectable(self.meta_shown(g), label), Act::Meta(g));
                }
            });
            ui.menu_button(t("menu-go"), |ui| {
                entry(ui, has_items, item(t("menu-next"), "→".into()), Act::Go(Go::Next));
                entry(ui, has_items, item(t("menu-prev"), "←".into()), Act::Go(Go::Prev));
                entry(ui, has_items, item(t("menu-first"), t("key-home")), Act::Go(Go::First));
                entry(ui, has_items, item(t("menu-last"), t("key-end")), Act::Go(Go::Last));
                ui.separator();
                entry(ui, can_go_up, item(t("menu-parent"), t("key-backspace")), Act::Up);
            });
            ui.menu_button(t("menu-help"), |ui| {
                entry(ui, true, item(t("menu-manual"), "F1".into()), Act::Manual);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("EyeCanSee {}", version()));
            });
        });
        if let Some(a) = &act {
            log!("menu {a:?}");
        }
        match act {
            Some(Act::Address) => ui.memory_mut(|m| m.request_focus(egui::Id::unique("address"))),
            Some(Act::Options) => self.options = true,
            Some(Act::Quit) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            Some(Act::Open) => self.activate(self.selected),
            Some(Act::Back) => self.close_viewer(),
            Some(Act::Refresh) => self.refresh(),
            Some(Act::Go(to)) => self.go(to),
            Some(Act::Up) => self.go_up(),
            Some(Act::Manual) => self.help = true,
            Some(Act::Meta(g)) => self.show_meta(g, !self.meta_shown(g)),
            Some(Act::Sort(by)) => self.set_sort(by, self.settings.sort_desc),
            Some(Act::SortDesc) => self.set_sort(self.settings.sort_by, !self.settings.sort_desc),
            None => {}
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if self.viewing.is_some() {
                if ui.button("⊞").on_hover_text(self.i18n.t("menu-back-to-grid")).clicked() {
                    self.close_viewer();
                }
            } else if ui
                .add_enabled(!self.dir.is_empty(), egui::Button::new("⬆"))
                .on_hover_text(self.i18n.t("tb-up"))
                .clicked()
            {
                self.go_up();
            }
            if ui.button("⟳").on_hover_text(self.i18n.t("menu-refresh")).clicked() {
                self.refresh();
            }
            // Metadata: closes all the boxes if one is open, otherwise reopens them all.
            if self.viewing.is_none() {
                let any = meta::Group::ALL.into_iter().any(|g| self.meta_shown(g));
                if ui.selectable_label(any, "ℹ").on_hover_text(self.i18n.t("tb-meta")).clicked() {
                    for g in meta::Group::ALL {
                        self.show_meta(g, !any);
                    }
                }
            }
            // Space to the right of the address: sorting (drop-down and direction) and thumbnail slider.
            let slider_width = if self.viewing.is_none() { 370.0 } else { 0.0 };
            let address = egui::TextEdit::singleline(&mut self.address)
                .id(egui::Id::unique("address"))
                .hint_text(self.i18n.t("tb-address-hint"))
                .desired_width(ui.available_width() - slider_width);
            let response = ui.add(address);
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let address = self.address.clone();
                self.open_source(ctx, &address);
            }
            if self.viewing.is_none() {
                let (mut by, desc) = (self.settings.sort_by, self.settings.sort_desc);
                egui::ComboBox::from_id_salt("sort")
                    .width(150.0)
                    .selected_text(self.i18n.t(by.label()))
                    .show_ui(ui, |ui| {
                        for b in settings::SortBy::ALL {
                            ui.selectable_value(&mut by, b, self.i18n.t(b.label()));
                        }
                    })
                    .response
                    .on_hover_text(self.i18n.t("sort-by"));
                let arrow = if desc { "▼" } else { "▲" };
                let flip = ui.button(arrow).on_hover_text(self.i18n.t(if desc { "sort-desc" } else { "sort-asc" })).clicked();
                if by != self.settings.sort_by || flip {
                    self.set_sort(by, desc != flip);
                }
                let hint = self.i18n.t("tb-thumb-size");
                ui.label("🖼").on_hover_text(&hint);
                ui.add(egui::Slider::new(&mut self.thumb_size, 64.0..=THUMB_SIDE as f32).show_value(false))
                    .on_hover_text(hint);
            }
        });
    }

    /// Changes the sort order: re-sorts the listings already read and restarts from the current
    /// folder (thumbnails and selection are by position), keeping the same image selected.
    fn set_sort(&mut self, by: settings::SortBy, desc: bool) {
        self.settings.sort_by = by;
        self.settings.sort_desc = desc;
        self.settings.save();
        let selected = self.cursor.then(|| self.selected.checked_sub(self.listing_dirs())).flatten();
        let path = selected.and_then(|i| self.files().get(i)).map(|f| f.path.clone());
        for l in self.listings.values_mut().flatten() {
            l.sort_files(by, desc);
        }
        self.set_dir(self.dir.clone());
        if let Some(path) = path
            && let Some(i) = self.files().iter().position(|f| f.path == path)
        {
            self.selected = self.listing_dirs() + i;
            self.cursor = true;
            self.scroll_to_selected = true;
        }
    }

    /// Number of subfolders of the current folder (they come before the images).
    fn listing_dirs(&self) -> usize {
        match self.listings.get(&self.dir) {
            Some(Ok(l)) => l.dirs.len(),
            _ => 0,
        }
    }

    fn meta_shown(&self, g: meta::Group) -> bool {
        !self.settings.meta_hidden.iter().any(|k| k == g.key())
    }

    /// Opens or closes a metadata box (X, menu, button) and remembers it.
    fn show_meta(&mut self, g: meta::Group, show: bool) {
        self.settings.meta_hidden.retain(|k| k != g.key());
        if !show {
            self.settings.meta_hidden.push(g.key().to_string());
        }
        self.settings.save();
    }

    /// How many folders and images there are, and name and size of the selected item.
    fn status_bar(&self, ui: &mut egui::Ui) {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        ui.horizontal(|ui| {
            ui.label(self.i18n.t_args("status-folders", &[("n", &l.dirs.len().to_string())]));
            ui.label(self.i18n.t_args("status-images", &[("n", &l.files.len().to_string())]));
            if !self.cursor {
                return;
            }
            ui.separator();
            match self.selected.checked_sub(l.dirs.len()).and_then(|i| l.files.get(i)) {
                Some(f) => {
                    ui.label(source::name(&f.path));
                    ui.weak(self.human_size(f.size));
                    if let Some(date) = self.file_date(&f.modified) {
                        ui.weak(date);
                    }
                }
                None => {
                    ui.label(l.dirs.get(self.selected).map_or("", |d| source::name(d)));
                }
            }
        });
    }

    /// Modification date in the computer's time zone, in the language's format. `modified` stays
    /// as it arrived (the thumbnail cache needs it): Unix seconds from phones, RFC 3339 from OpenDAL.
    fn file_date(&self, modified: &str) -> Option<String> {
        let local = parse_modified(modified)?.to_zoned(jiff::tz::TimeZone::system());
        Some(local.strftime(&self.i18n.t("date-format")).to_string())
    }

    /// 850 KB, 3.9 MB (the language's decimal separator).
    fn human_size(&self, bytes: u64) -> String {
        let kb = bytes as f64 / 1024.0;
        let s = if kb < 1024.0 { format!("{kb:.0} KB") } else { format!("{:.1} MB", kb / 1024.0) };
        s.replace('.', &self.i18n.t("decimal-separator"))
    }

    fn viewer(&mut self, ui: &mut egui::Ui, current: usize) {
        ui.centered_and_justified(|ui| match self.full.get(&current) {
            Some(Ok(tex)) => {
                let response = ui.add(egui::Image::new(tex).shrink_to_fit().sense(egui::Sense::click()));
                if response.double_clicked() {
                    self.close_viewer();
                }
            }
            Some(Err(e)) => {
                let name = self.files().get(current).map_or("", |f| source::name(&f.path));
                ui.label(browser::error_text(ui, self.i18n.t_args("cannot-load", &[("file", name), ("error", e)])));
            }
            None => {
                ui.spinner();
            }
        });
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.viewing.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        raw_input.events.retain(|e| match e {
            egui::Event::Key { key: Key::Tab, pressed, modifiers, .. }
                if modifiers.is_none() || modifiers.matches_exact(egui::Modifiers::SHIFT) =>
            {
                if *pressed {
                    self.tab = Some(modifiers.shift);
                }
                false
            }
            _ => true,
        });
    }

    fn on_exit(&mut self) {
        // Always saved, so if "remember" is chosen later there is already a size.
        if self.loader.is_some() {
            self.settings.last_location = Some(self.display(&self.dir));
        }
        self.settings.save();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        self.retry(&ctx);
        self.global_keys(&ctx);
        if self.viewing.is_some() {
            self.viewer_keys(&ctx);
        }
        if let Some(current) = self.viewing {
            self.prefetch_full(current);
        }
        self.update_title(&ctx);
        // Size to remember: that of the non-maximised window.
        let (size, maximized) = ctx.input(|i| (i.viewport_rect().size(), i.viewport().maximized));
        self.settings.window_maximized = maximized.unwrap_or(false);
        if !self.settings.window_maximized {
            self.settings.window_size = Some([size.x, size.y]);
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, &ctx));
        if let Some(e) = self.error.clone() {
            egui::Panel::bottom("error").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(browser::error_text(ui, e));
                    if ui.small_button("✖").clicked() {
                        self.error = None;
                    }
                });
            });
        }
        match self.viewing {
            Some(current) => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(ui.style()).fill(Color32::from_gray(18)))
                    .show(ui, |ui| self.viewer(ui, current));
            }
            None => {
                egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
                if meta::Group::ALL.into_iter().any(|g| self.meta_shown(g)) {
                    egui::Panel::bottom("meta").show(ui, |ui| self.meta_panel(ui));
                }
                egui::Panel::left("tree").resizable(true).default_size(240.0).show(ui, |ui| self.tree(ui));
                let canvas = theme::canvas(ui.visuals());
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(ui.style()).fill(canvas))
                    .show(ui, |ui| self.grid(ui));
                if let Some(back) = self.tab.take() {
                    self.tab(&ctx, back);
                }
            }
        }

        let mut open = self.help;
        egui::Window::new(self.i18n.t("menu-manual"))
            .open(&mut open)
            .default_size([560.0, 480.0])
            .show(&ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    CommonMarkViewer::new().show(ui, &mut self.help_cache, self.i18n.manual);
                });
            });
        self.help = open;
        self.options_window(&ctx);

        #[cfg(debug_assertions)]
        debug_screenshot(&ctx);
    }
}

/// Debug only: `EYECANSEE_SCREENSHOT=file.png` saves the window after 3 seconds
/// (`EYECANSEE_SCREENSHOT_DELAY=<seconds>` to change them) and exits.
/// Used to check the interface without capturing the screen from outside (blocked on Wayland).
#[cfg(debug_assertions)]
fn debug_screenshot(ctx: &egui::Context) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REQUESTED: AtomicBool = AtomicBool::new(false);
    let Ok(path) = std::env::var("EYECANSEE_SCREENSHOT") else { return };
    let delay = std::env::var("EYECANSEE_SCREENSHOT_DELAY").ok().and_then(|s| s.parse().ok()).unwrap_or(3.0);
    if ctx.input(|i| i.time) > delay && !REQUESTED.swap(true, Ordering::Relaxed) {
        let ctx2 = ctx.clone();
        ctx.request_screenshot(move |img| {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let [w, h] = img.size;
            image::save_buffer(&path, &rgba, w as u32, h as u32, image::ColorType::Rgba8).expect("screenshot");
            ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
        });
    }
    ctx.request_repaint();
}

fn main() -> eframe::Result {
    let arg = std::env::args().nth(1);
    let settings = Settings::load();
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_app_id("eyecansee");
    // Title bar and task bar (without it, eframe puts its own). On Windows the icon of the
    // .exe file is something else: `build.rs` embeds it.
    if let Some(icon) = decode::app_icon(256) {
        viewport = viewport.with_icon(egui::IconData { rgba: icon.into_raw(), width: 256, height: 256 });
    }
    match settings.window_start {
        WindowStart::Normal => {}
        WindowStart::Maximized => viewport = viewport.with_maximized(true),
        WindowStart::Remember => {
            if let Some(size) = settings.window_size {
                viewport = viewport.with_inner_size(size);
            }
            viewport = viewport.with_maximized(settings.window_maximized);
        }
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native("EyeCanSee", options, Box::new(move |cc| Ok(Box::new(App::new(&cc.egui_ctx, settings, arg)))))
}

/// `File::modified` as an instant: Unix seconds (phones) or RFC 3339 (OpenDAL); "" = no date.
fn parse_modified(modified: &str) -> Option<jiff::Timestamp> {
    match modified.parse::<i64>() {
        Ok(secs) => jiff::Timestamp::from_second(secs).ok(),
        Err(_) => modified.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn modified_formats() {
        let unix = super::parse_modified("1776333600").unwrap();
        let rfc = super::parse_modified("2026-04-16T10:00:00Z").unwrap();
        assert_eq!(unix, rfc);
        assert!(super::parse_modified("2026-04-16T10:00:00.123456789Z").is_some());
        assert!(super::parse_modified("").is_none());
    }

    #[test]
    fn version_matches_cargo() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"), "VERSION and Cargo.toml must match");
    }
}
