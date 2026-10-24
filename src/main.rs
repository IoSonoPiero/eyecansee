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
mod convert;
mod decode;
mod devices;
mod i18n;
mod icons;
mod loader;
mod meta;
mod mtp;
mod save_dialogs;
mod settings;
mod single;
mod source;
mod theme;
mod thumbcache;
mod transfer;
mod transform;
mod update;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Key, TextureHandle, TextureOptions};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use i18n::I18n;
use loader::{Loader, Msg, THUMB_SIDE};
use save_dialogs::md;
use settings::{RemoteDelete, Settings, ThemeChoice, WindowStart};
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

/// Items of the menu bar and of the right-click menus: the menus only record the choice, which
/// runs afterwards (`App::run`) with access to the whole app.
#[derive(Debug)]
pub(crate) enum Act {
    /// A submenu of a right-click menu (Transform): never run itself.
    Submenu(Vec<browser::MenuItem>),
    Address,
    Options,
    /// Item (`None` = the selected one or the one in the viewer); `true`: for good (Shift+Del).
    Delete(Option<String>, bool),
    Rename(Option<String>),
    /// "Save in another format" for an image (`None` = the selected one or the one in the viewer).
    Convert(Option<String>),
    /// Makes the thumbnails again (`None` = the selected images or the one in the viewer).
    RedoThumbs(Option<String>),
    /// Shift+F5: every thumbnail of the open folder made again, and the folder read again.
    RedoAllThumbs,
    /// A flip or quarter turn: in the viewer shown until saved; in the grid the item (`None` = the
    /// selected or marked images) saved straight away, after a confirmation.
    Transform(transform::Op, Option<String>),
    /// The turned image saved over the original (after a confirmation).
    Save,
    /// Puts back from the trash an item, or everything if it is a trash folder (`None` = selected).
    Restore(Option<String>),
    Copy(Option<String>),
    Cut(Option<String>),
    /// Into a folder (`None` = the one open).
    Paste(Option<String>),
    Quit,
    /// Install the downloaded update and start EyeCanSee again.
    Update,
    /// The selected grid item.
    Open,
    /// A folder of the tree.
    OpenDir(String),
    Back,
    Refresh,
    Go(Go),
    Up,
    Manual,
    CheckUpdates,
    About,
    Meta(meta::Group),
    Sort(settings::SortBy),
    SortDesc,
}

/// Image > Transform, as GIMP orders it: flips, then turns. Label and keys (Fluent ids).
pub(crate) fn transform_items() -> [(transform::Op, &'static str, Option<&'static str>); 5] {
    use transform::Op::*;
    [
        (FlipH, "tf-flip-h", None),
        (FlipV, "tf-flip-v", None),
        (Cw, "tf-rotate-cw", Some("key-rotate-cw")),
        (Ccw, "tf-rotate-ccw", Some("key-rotate-ccw")),
        (Half, "tf-rotate-180", None),
    ]
}

/// A paste waiting for answers about names already in the destination.
struct PasteAsk {
    dir: String,
    /// Still to ask; `None` until the loader has looked.
    conflicts: Option<Vec<String>>,
    choices: HashMap<String, transfer::Choice>,
    /// "Apply to all".
    all: bool,
}

/// Tabs of the Options window.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum OptTab {
    #[default]
    General,
    Look,
    Browsing,
    Deleting,
    Thumbnails,
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
    /// The ".." cell is selected (only while `cursor` is false: nothing else is, so Delete and
    /// Rename have no target).
    up_selected: bool,
    /// Items marked with Ctrl+click, Shift+click, Shift+arrows or Ctrl+A (paths): when there are
    /// any, they are the selection shown and Delete acts on all of them. `anchor`: where a Shift
    /// range starts.
    marked: HashSet<String>,
    anchor: usize,
    /// Rectangle being dragged on the grid: corner where it started (grid coordinates, so it
    /// follows scrolling) and the marks there were already (kept with Ctrl).
    band: Option<(egui::Pos2, HashSet<String>)>,
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
    /// Tallest height of the metadata panel so far: it never shrinks (see `meta_panel`).
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
    /// Zoom of the viewer in screen pixels per image pixel (`None` = fitted to the window), for the
    /// image `zoom_of`; `pan` = centre of the view in image pixels. A new image starts fitted.
    zoom: Option<f32>,
    zoom_of: Option<usize>,
    /// Flips and quarter turns of the open image, not saved yet (Image > Transform); they go
    /// with the zoom when another image opens.
    turn: transform::Turn,
    /// "Save" asked and prepared: the image, its turn, the file to write and whether it lost
    /// quality, for the confirmation of the overwrite.
    save_ready: Option<(String, transform::Turn, Arc<Vec<u8>>, bool)>,
    /// Images being prepared or saved after a turn: not saved again until done (status bar).
    saving: HashSet<String>,
    /// Names left out of a "Save" on the grid (animated…), shown once every save is done.
    skipped: Vec<String>,
    /// Images saved by transforms on the grid, for the summary once every save is done.
    batch_saved: Option<usize>,
    /// A message in a window (a "Save" refused, how a grid save went); `true` offers "Save as…".
    notice: Option<(String, bool)>,
    /// File > Open location: the window, with what is typed in it.
    open_address: Option<AddressForm>,
    /// A transform on images of the grid: the images and the operation, to confirm.
    batch_turn: Option<(Vec<String>, transform::Op)>,
    pan: egui::Vec2,
    /// Zoom shown, for the status bar.
    zoom_shown: f32,
    /// Wheel turned towards the next image, in notches, when the wheel changes image.
    wheel_notches: f32,
    /// Viewer images already requested from the loader.
    pending: HashSet<usize>,
    thumb_size: f32,
    address: String,
    error: Option<String>,
    /// New version downloaded at startup, waiting for "Install and restart".
    update: Option<update::Ready>,
    /// Help > Check for updates: what to show (`None` = window closed).
    update_note: Option<String>,
    /// "Save in another format" window.
    convert: Option<convert::Window>,
    title: String,
    help: bool,
    about: bool,
    /// App icon for the About window, made the first time it opens.
    about_icon: Option<TextureHandle>,
    help_cache: CommonMarkCache,
    settings: Settings,
    devices: Vec<devices::Device>,
    /// The tree must open down to the current folder (after every folder change).
    reveal: bool,
    /// Options window open, and its tab.
    options: bool,
    options_tab: OptTab,
    /// Confirmation of emptying the cache in progress: (thumbnails, bytes).
    purge_confirm: Option<(u64, u64)>,
    /// Sources already gone through cache maintenance in this session.
    maintained: HashSet<String>,
    /// Item waiting for the user to confirm its deletion: path, size (images only), what deleting
    /// it does, on an NTFS disk (warning: Nautilus does not restore from there).
    delete_confirm: Option<(Vec<String>, Option<u64>, source::Deletion, bool)>,
    /// Items to put back from the trash, one at a time; one is on its way (`restore_busy`), or its
    /// name is taken and the user is asked what to do (`restore_conflict`, its path in the trash).
    restore_queue: std::collections::VecDeque<String>,
    restore_busy: bool,
    restore_conflict: Option<String>,
    /// Ctrl+C / Ctrl+X.
    clip: Option<transfer::Clip>,
    paste_ask: Option<PasteAsk>,
    /// Copy running: progress, move, source (`base`) and folder where it goes.
    transfer: Option<(Arc<transfer::Progress>, bool, String, String)>,
    /// When the folders filled by the copy were last read again.
    touched_at: f64,
    /// Items whose deletion is under way, for the status bar.
    deleting: usize,
    /// Window closed during a copy: it closes when the copy has stopped.
    quit_after_transfer: bool,
    /// Rename in progress: name being written, path, first frame (focus and selection to set).
    rename: Option<(String, String, bool)>,
    /// Enter (true) or Esc (false) pressed while a confirmation is open (deletion, name taken).
    delete_answer: Option<bool>,
    /// The open local folder, watched for changes made outside EyeCanSee.
    watcher: Option<notify::RecommendedWatcher>,
    watched: Option<std::path::PathBuf>,
    /// Folder changed on disk: when to read it again (egui time), so a burst of events
    /// (a big file being copied) gives one listing every 0.3 s.
    changed_at: Option<f64>,
    /// Folders being read again after a change: their new listing replaces the old one in place.
    relisting: HashSet<String>,
    /// Item chosen from a right-click menu, run when the frame is drawn (never during the grid).
    act: Option<Act>,
}

impl App {
    /// `arg`: address from the command line. Without it (e.g. started from the applications menu)
    /// the last folder viewed if the option is on, otherwise the home folder.
    fn new(ctx: &egui::Context, settings: Settings, arg: Option<String>) -> Self {
        ctx.set_fonts(fonts());
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
            up_selected: false,
            marked: HashSet::new(),
            band: None,
            anchor: 0,
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
            zoom: None,
            zoom_of: None,
            turn: Default::default(),
            save_ready: None,
            saving: HashSet::new(),
            skipped: Vec::new(),
            batch_saved: None,
            notice: None,
            open_address: None,
            batch_turn: None,
            pan: egui::Vec2::ZERO,
            zoom_shown: 1.0,
            wheel_notches: 0.0,
            pending: HashSet::new(),
            thumb_size: 160.0,
            address: String::new(),
            error: None,
            title: String::new(),
            help: false,
            help_cache: CommonMarkCache::default(),
            about: false,
            clip: None,
            paste_ask: None,
            transfer: None,
            touched_at: 0.0,
            quit_after_transfer: false,
            deleting: 0,
            about_icon: None,
            settings,
            reveal: true,
            // Straight away, not only from the thread: the fallback already needs it if the first address is a busy mtp://.
            devices: devices::scan(),
            options: false,
            options_tab: OptTab::General,
            purge_confirm: None,
            update: None,
            update_note: None,
            convert: None,
            maintained: HashSet::new(),
            delete_confirm: None,
            delete_answer: None,
            rename: None,
            restore_queue: Default::default(),
            restore_busy: false,
            restore_conflict: None,
            watcher: None,
            watched: None,
            changed_at: None,
            relisting: HashSet::new(),
            act: None,
        };
        app.watcher = {
            let (tx, ctx) = (app.tx.clone(), ctx.clone());
            notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
                if e.is_ok_and(|e| !e.kind.is_access()) && tx.send(Msg::Changed).is_ok() {
                    ctx.request_repaint();
                }
            })
            .inspect_err(|e| log!("folder watcher: ERROR {e}"))
            .ok()
        };
        theme::apply(ctx, app.settings.theme);
        devices::watch(app.tx.clone(), ctx.clone());
        single::listen(app.tx.clone(), ctx.clone());
        if app.settings.check_updates {
            app.check_updates(ctx);
        }
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
        log!("open source {:?}", without_password(address));
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
        let (wrap, wrap_hint) = (t("opt-wrap-around"), t("opt-wrap-around-hint"));
        let (wheel, wheel_zoom, wheel_next, wheel_hint) = (t("opt-wheel"), t("opt-wheel-zoom"), t("opt-wheel-next"), t("opt-wheel-hint"));
        let (max_zoom, max_zoom_hint) = (t("opt-max-zoom"), t("opt-max-zoom-hint"));
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
        let (local_permanent, local_permanent_hint) = (t("opt-local-permanent"), t("opt-local-permanent-hint"));
        let (updates, updates_hint) = (t("opt-check-updates"), t("opt-check-updates-hint"));
        let (remote_delete, remote_folder, remote_permanent) =
            (t("opt-remote-delete"), t("opt-remote-delete-folder"), t("opt-remote-delete-permanent"));
        let (purge, purge_no) = (t("opt-purge"), t("opt-purge-no"));
        let purge_text = self.purge_confirm.map(|(n, bytes)| {
            let mb = format!("{:.1}", bytes as f64 / 1_048_576.0);
            (self.i18n.t_args("opt-purge-confirm", &[("n", &n.to_string()), ("mb", &mb)]), self.i18n.t("opt-purge-yes"))
        });
        let mut purged = false;
        let tab_names = [
            (OptTab::General, t("opt-tab-general")),
            (OptTab::Look, t("opt-tab-look")),
            (OptTab::Browsing, t("opt-tab-browsing")),
            (OptTab::Deleting, t("opt-tab-deleting")),
            (OptTab::Thumbnails, t("opt-tab-thumbnails")),
        ];
        let tab = &mut self.options_tab;
        let confirm = &mut self.purge_confirm;
        let s = &mut self.settings;
        // Fixed id: changing language changes the title, and the window must not move.
        // One tab per subject, as in other programs: the window stays short. Fixed size and in the
        // middle, so changing tab does not make it jump.
        egui::Window::new(title)
            .id(egui::Id::unique("options"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([460.0, 330.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // As tall as the longest tab (General), so changing tab does not resize the window.
                ui.set_min_height(330.0);
                ui.horizontal(|ui| {
                    for (value, name) in tab_names {
                        ui.selectable_value(tab, value, name);
                    }
                });
                ui.separator();
                match tab {
                    OptTab::General => {
                        ui.checkbox(&mut s.remember_location, remember).on_hover_text(remember_hint);
                        ui.add_space(6.0);
                        ui.label(window);
                        ui.radio_value(&mut s.window_start, WindowStart::Normal, normal);
                        ui.radio_value(&mut s.window_start, WindowStart::Maximized, maximized);
                        ui.radio_value(&mut s.window_start, WindowStart::Remember, remember_size);
                        ui.add_space(6.0);
                        ui.label(language_label);
                        ui.radio_value(&mut s.language, None, language_system);
                        for (code, name) in languages {
                            ui.radio_value(&mut s.language, Some(code.to_string()), name);
                        }
                        ui.add_space(6.0);
                        ui.checkbox(&mut s.check_updates, updates).on_hover_text(updates_hint);
                    }
                    OptTab::Look => {
                        ui.label(theme_label);
                        ui.radio_value(&mut s.theme, ThemeChoice::System, theme_system);
                        ui.radio_value(&mut s.theme, ThemeChoice::Light, theme_light);
                        ui.radio_value(&mut s.theme, ThemeChoice::Dark, theme_dark);
                    }
                    OptTab::Browsing => {
                        ui.checkbox(&mut s.remember_scroll, scroll).on_hover_text(scroll_hint);
                        ui.checkbox(&mut s.wrap_around, wrap).on_hover_text(wrap_hint);
                        ui.add_space(6.0);
                        ui.label(wheel).on_hover_text(&wheel_hint);
                        ui.radio_value(&mut s.wheel_zoom, true, wheel_zoom).on_hover_text(&wheel_hint);
                        ui.radio_value(&mut s.wheel_zoom, false, wheel_next).on_hover_text(&wheel_hint);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(max_zoom).on_hover_text(&max_zoom_hint);
                            ui.add(egui::DragValue::new(&mut s.max_zoom).range(100..=10_000).speed(10).suffix("%")).on_hover_text(&max_zoom_hint);
                        });
                    }
                    OptTab::Deleting => {
                        ui.checkbox(&mut s.local_permanent, local_permanent).on_hover_text(local_permanent_hint);
                        ui.add_space(6.0);
                        ui.label(remote_delete);
                        ui.radio_value(&mut s.remote_delete, RemoteDelete::TrashFolder, remote_folder);
                        ui.radio_value(&mut s.remote_delete, RemoteDelete::Permanent, remote_permanent);
                    }
                    OptTab::Thumbnails => {
                        ui.checkbox(&mut s.thumb_maintenance, maint).on_hover_text(maint_hint);
                        ui.add_enabled_ui(s.thumb_maintenance, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(limit);
                                ui.add(egui::DragValue::new(&mut s.thumb_cache_mb).range(16..=1_000_000).suffix(" MB"));
                            });
                        });
                        ui.add_space(6.0);
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
        shown(&self.base, dir)
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
        self.up_selected = false;
        self.marked.clear();
        self.anchor = 0;
        self.scroll_top = true;
        self.meta = None;
        self.meta_asked = None;
        self.retry_at = None;
        self.viewing = None;
        self.start = None;
        let dir = self.dir.clone();
        self.request_listing(&dir);
        self.watch();
    }

    /// Watches the open folder if it is on this computer. Phones and servers send nothing (F5);
    /// gvfs mounts neither: changes on the phone do not reach inotify.
    fn watch(&mut self) {
        use notify::Watcher;
        let Some(watcher) = &mut self.watcher else { return };
        if let Some(old) = self.watched.take() {
            let _ = watcher.unwatch(&old);
        }
        self.changed_at = None;
        if self.base.contains("://") || source::in_gvfs(&self.base, &self.dir) {
            return;
        }
        let path: std::path::PathBuf =
            std::iter::once(self.base.as_str()).chain(self.dir.split('/').filter(|s| !s.is_empty())).collect();
        match watcher.watch(&path, notify::RecursiveMode::NonRecursive) {
            Ok(()) => self.watched = Some(path),
            Err(e) => log!("watch {path:?}: ERROR {e}"),
        }
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
                    match r {
                        Ok(l) if self.relisting.remove(&dir) => self.swap_listing(&dir, l),
                        r => {
                            self.relisting.remove(&dir);
                            self.listings.insert(dir, r);
                        }
                    }
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
                // Counted from the first event, not the last: a copy into a subfolder sends one for
                // every file, and the folder was read again only when the copy ended.
                Msg::Changed if self.changed_at.is_none() => self.changed_at = Some(ctx.input(|i| i.time) + 0.3),
                Msg::Changed => {}
                Msg::Deleted(path, how, Ok(())) => {
                    log!("deleted {path:?}");
                    // An emptied trash going away comes as one more: never below zero.
                    self.deleting = self.deleting.saturating_sub(1);
                    self.remove_item(&path, how == source::Deletion::TrashFolder);
                }
                Msg::TrashItems(paths) => {
                    log!("restore all: {} items", paths.len());
                    self.restore_queue.extend(paths);
                }
                Msg::Restored(path, Ok(to)) => {
                    log!("restored {path:?} -> {to:?}");
                    self.restore_busy = false;
                    self.listings.retain(|k, _| !(to.ends_with('/') && k.starts_with(&to)));
                    self.remove_item(&path, false);
                    self.relist(source::parent(&to));
                }
                Msg::Restored(path, Err(("error-exists", _))) => {
                    log!("restore {path:?}: name taken");
                    self.restore_busy = false;
                    self.restore_conflict = Some(path);
                }
                Msg::Restored(path, Err((id, detail))) => {
                    log!("restore {path:?}: ERROR {id} {detail}");
                    self.restore_busy = false;
                    self.restore_queue.clear();
                    self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                }
                Msg::Renamed(old, Ok(new)) => {
                    log!("renamed {old:?} -> {new:?}");
                    self.renamed(&old, &new);
                }
                Msg::Renamed(old, Err((id, detail))) => {
                    log!("rename {old:?}: ERROR {id} {detail}");
                    self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                }
                Msg::PasteCheck(Ok(conflicts)) => {
                    log!("paste: {} names taken", conflicts.len());
                    if let Some(ask) = &mut self.paste_ask {
                        ask.conflicts = Some(conflicts);
                    }
                }
                Msg::PasteCheck(Err((id, detail))) => {
                    log!("paste: ERROR {id} {detail}");
                    self.paste_ask = None;
                    self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                }
                Msg::Pasted(gone, r) => {
                    if self.quit_after_transfer {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    let Some((p, _, base, dir)) = self.transfer.take() else { continue };
                    log!(
                        "pasted: {} moved away, {r:?}; {} files, {} bytes, {:.1} s looking inside folders",
                        gone.len(),
                        p.files.load(Ordering::Relaxed),
                        p.done.load(Ordering::Relaxed),
                        p.looked.lock().unwrap()
                    );
                    if base == self.base {
                        for path in &gone {
                            self.remove_item(path, false);
                        }
                        self.listings.retain(|k, _| !(k.starts_with(&dir) && *k != dir));
                        self.relist(dir);
                    }
                    if let Err((id, detail)) = r
                        && id != "error-cancelled"
                    {
                        self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                    }
                }
                Msg::Deleted(path, _, Err((id, detail))) => {
                    log!("delete {path:?}: ERROR {id} {detail}");
                    // The loader stops at the first error: the others will not come.
                    self.deleting = 0;
                    self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                }
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
                Msg::Open(address) => {
                    log!("handed over: {:?}", without_password(&address));
                    if !address.is_empty() {
                        self.open_source(ctx, &address);
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                // Shown in a window only when asked from the Help menu (the window is open).
                Msg::Original(r) => self.convert_original(ctx, r),
                Msg::Encoded(generation, r) => self.convert_encoded(ctx, generation, r),
                Msg::Saved(r) => self.convert_saved(r),
                Msg::Prepared(path, turn, r) => {
                    self.saving.remove(&path);
                    match r {
                        // Only for the image still open with the same turn: the confirmation is about it.
                        Ok((data, lossy)) if self.viewed(&path) && turn == self.turn => {
                            self.save_ready = Some((path, turn, data, lossy));
                        }
                        Ok(_) => {}
                        Err((loader::SKIPPED, name)) => {
                            // Say exactly what would be lost.
                            let id = match convert::Format::of_file(&name) {
                                Some(convert::Format::Tiff) => "save-pages",
                                Some(convert::Format::Jpeg) => "save-extra",
                                Some(_) => "save-animated",
                                None => loader::SKIPPED,
                            };
                            self.notice = Some((self.i18n.t_args(id, &[("detail", &md(&name))]), self.viewed(&path)));
                        }
                        Err((id, detail)) => {
                            log!("save: ERROR {id} {detail}");
                            self.error = Some(self.i18n.t_args("save-failed", &[("detail", &self.i18n.t_args(id, &[("detail", &detail)]))]));
                        }
                    }
                }
                Msg::Replaced(path, r) => {
                    self.saving.remove(&path);
                    match r {
                        Ok(()) => {
                            log!("saved {path:?}");
                            if let Some(n) = &mut self.batch_saved {
                                *n += 1;
                            }
                            // Same name, maybe the same size: what is in memory is the old image.
                            if let Some(i) = self.files().iter().position(|f| f.path == path) {
                                self.full.remove(&i);
                            }
                            if self.viewed(&path) {
                                self.turn = Default::default();
                            }
                            self.relist(source::parent(&path));
                            self.redo_thumbs(ctx, vec![path]);
                        }
                        Err((loader::SKIPPED, name)) => self.skipped.push(name),
                        Err((id, detail)) => {
                            log!("save: ERROR {id} {detail}");
                            self.error = Some(self.i18n.t_args("save-failed", &[("detail", &self.i18n.t_args(id, &[("detail", &detail)]))]));
                        }
                    }
                    if self.saving.is_empty()
                        && let Some(n) = self.batch_saved.take()
                    {
                        let mut text = self.i18n.t_args("batch-done", &[("n", &n.to_string())]);
                        if !self.skipped.is_empty() {
                            let names: String = std::mem::take(&mut self.skipped).iter().map(|n| format!("\n- {}", md(n))).collect();
                            text = format!("{text}\n\n{}\n{names}", self.i18n.t("batch-skipped-names"));
                        }
                        self.notice = Some((text, false));
                    }
                }
                Msg::Forgotten(paths) => {
                    let paths: HashSet<String> = paths.into_iter().collect();
                    let Some(Ok(l)) = self.listings.get(&self.dir) else { continue };
                    let gone: Vec<usize> = l.files.iter().enumerate().filter(|(_, f)| paths.contains(&f.path)).map(|(i, _)| i).collect();
                    for i in gone {
                        self.thumbs.remove(&i);
                        self.generated.remove(&i);
                    }
                    self.thumb_queue_key = None;
                }
                Msg::Update(r) => {
                    log!("update: {r:?}");
                    let note = match &r {
                        Ok(Some(ready)) => self.i18n.t_args("update-found", &[("version", &ready.version)]),
                        Ok(None) => self.i18n.t_args("update-latest", &[("version", version())]),
                        Err(e) => self.i18n.t_args("update-check-failed", &[("detail", e)]),
                    };
                    if self.update_note.is_some() {
                        self.update_note = Some(note);
                    }
                    if let Ok(Some(ready)) = r {
                        self.update = Some(ready);
                    }
                }
                Msg::Devices(d) => {
                    log!("devices: {:?}", d.iter().map(|d| &d.path).collect::<Vec<_>>());
                    // Open phone unplugged: it disappears altogether, back to home as at startup
                    // (otherwise it stayed in the tree with the folders read before).
                    let serial = self.base.strip_prefix("mtp://").map(|s| s.trim_end_matches('/').to_string());
                    let has = |devices: &[devices::Device], s: &str| devices.iter().any(|d| d.serial.as_deref() == Some(s));
                    let unplugged = serial.is_some_and(|s| has(&self.devices, &s) && !has(&d, &s));
                    self.devices = d;
                    if unplugged {
                        log!("phone unplugged");
                        let home = std::env::home_dir().map_or(".".into(), |h| h.to_string_lossy().into_owned());
                        self.open_source(ctx, &home);
                    }
                }
            }
        }
    }

    /// Phone without storage (locked, charging or not yet in "File transfer") or not found (it
    /// reconnects when switched to "File transfer", or was unplugged): the folder is
    /// read again by itself every `RETRY_SECS`, without F5. Until then the message stays.
    /// Reads the open folder again after a change on disk, once the events have calmed down.
    fn reread_changed(&mut self, ctx: &egui::Context) {
        let Some(at) = self.changed_at else { return };
        let now = ctx.input(|i| i.time);
        if now < at {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            return;
        }
        let Some(loader) = &self.loader else { return };
        // A listing already on its way may have been read before the change: try again after it.
        if !self.listing_pending.insert(self.dir.clone()) {
            self.changed_at = Some(now + 0.3);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(0.3));
            return;
        }
        self.changed_at = None;
        log!("changed on disk {:?}", self.dir);
        self.relisting.insert(self.dir.clone());
        loader.list(self.dir.clone());
    }

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
        let (help, refresh, delete, shift, rename) = ctx.input(|i| {
            (i.key_pressed(Key::F1), i.key_pressed(Key::F5), i.key_pressed(Key::Delete), i.modifiers.shift, i.key_pressed(Key::F2))
        });
        if !ctx.egui_wants_keyboard_input() && self.rename.is_none() {
            if delete {
                self.ask_delete(None, shift);
            }
            if rename {
                self.ask_rename(None);
            }
        }
        if help {
            self.help = !self.help;
        }
        if refresh && shift {
            self.redo_all_thumbs(ctx);
        } else if refresh {
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
        let (cw, ccw, save, save_copy) = ctx.input_mut(|i| {
            let shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
            // With Shift first: Ctrl+R would match Ctrl+Shift+R too.
            let ccw = i.consume_key(shift, Key::R);
            let save_copy = i.consume_key(shift, Key::S);
            (i.consume_key(egui::Modifiers::COMMAND, Key::R), ccw, i.consume_key(egui::Modifiers::COMMAND, Key::S), save_copy)
        });
        if cw {
            self.act = Some(Act::Transform(transform::Op::Cw, None));
        } else if ccw {
            self.act = Some(Act::Transform(transform::Op::Ccw, None));
        } else if save {
            self.act = Some(Act::Save);
        } else if save_copy {
            self.act = Some(Act::Convert(None));
        }
    }

    /// The images (not folders) an action on `path` in the grid works on, as `chosen`.
    fn grid_images(&self, path: Option<String>) -> Vec<String> {
        if self.viewing.is_some() {
            return Vec::new();
        }
        self.chosen(path).into_iter().filter(|p| !p.ends_with('/') && !source::in_trash(p)).collect()
    }

    /// Next, previous, first, last: same action from the keyboard and from the Go menu.
    /// In the viewer it changes image (after the last one back to the first only with `wrap_around`,
    /// otherwise it stops there), in the grid it moves the selection.
    pub(crate) fn go(&mut self, to: Go) {
        if let Some(current) = self.viewing {
            let n = self.files().len();
            if n > 0 {
                self.viewing = Some(match to {
                    Go::Next if self.settings.wrap_around => (current + 1) % n,
                    Go::Prev if self.settings.wrap_around => (current + n - 1) % n,
                    Go::Next => (current + 1).min(n - 1),
                    Go::Prev => current.saturating_sub(1),
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

    /// The item Delete and Rename act on: the image in the viewer, or the item selected in the grid.
    fn target(&self) -> Option<String> {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return None };
        let path = match self.viewing {
            Some(i) => &l.files.get(i)?.path,
            None if self.cursor && self.focus == Focus::Grid => match l.dirs.get(self.selected) {
                Some(d) => d,
                None => &l.files.get(self.selected - l.dirs.len())?.path,
            },
            None => return None,
        };
        Some(path.clone()).filter(|p| self.can_change(p))
    }

    /// Never the root of the source, nor the storages of a phone.
    fn can_change(&self, path: &str) -> bool {
        !path.is_empty() && !(self.base.starts_with("mtp://") && path.ends_with('/') && source::parent(path).is_empty())
    }

    /// Path of a grid item (folders first, then images).
    pub(crate) fn item_path(&self, i: usize) -> Option<String> {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return None };
        l.dirs.get(i).or_else(|| l.files.get(i.checked_sub(l.dirs.len())?).map(|f| &f.path)).cloned()
    }

    /// The marked items still in the folder, in the grid's order.
    fn marked_paths(&self) -> Vec<String> {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return Vec::new() };
        l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).filter(|p| self.marked.contains(*p)).cloned().collect()
    }

    /// Total size of the images among `paths` (folders are not counted).
    fn images_size(&self, paths: &[String]) -> u64 {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return 0 };
        l.files.iter().filter(|f| paths.contains(&f.path)).map(|f| f.size).sum()
    }

    /// `path`: `None` = `target`, or all the marked items if there are any (also when `path` is
    /// one of them: the grid's context menu). `permanent`: Shift+Del, for good this time whatever
    /// the options say.
    /// What an action on `path` works on: all the marked items if it is one of them (`None` with
    /// marks in the grid too), otherwise just that one (`None` = the selected one or the one in the viewer).
    fn chosen(&self, path: Option<String>) -> Vec<String> {
        let marked = if self.viewing.is_none() { self.marked_paths() } else { Vec::new() };
        let paths: Vec<String> = match path {
            Some(p) if marked.contains(&p) => marked,
            None if !marked.is_empty() && self.focus == Focus::Grid => marked,
            p => p.or_else(|| self.target()).into_iter().collect(),
        };
        paths.into_iter().filter(|p| self.can_change(p)).collect()
    }

    /// Thumbnails gone from the disk cache, then from memory (`Msg::Forgotten`): the grid asks for
    /// them again and they are made from the file (a wrong one stays otherwise, since size and
    /// date do not change).
    fn redo_thumbs(&mut self, ctx: &egui::Context, paths: Vec<String>) {
        let (tx, ctx, back) = (self.tx.clone(), ctx.clone(), paths.clone());
        thumbcache::forget(&self.base, paths, move || {
            let _ = tx.send(Msg::Forgotten(back));
            ctx.request_repaint();
        });
    }

    fn redo_all_thumbs(&mut self, ctx: &egui::Context) {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        self.redo_thumbs(ctx, l.files.iter().map(|f| f.path.clone()).collect());
        self.refresh();
    }

    /// Ctrl+C / Ctrl+X: remembers the items and their source. The paths also go on the system
    /// clipboard as text: egui turns Ctrl+V into a paste only when the clipboard has something.
    fn clip(&mut self, ctx: &egui::Context, path: Option<String>, cut: bool) {
        let paths = self.chosen(path);
        let Some(loader) = &self.loader else { return };
        if paths.is_empty() || paths.iter().any(|p| source::is_trash(p) || source::in_trash(p)) {
            return;
        }
        log!("clip {paths:?} cut {cut}");
        ctx.copy_text(paths.iter().map(|p| self.display(p)).collect::<Vec<_>>().join("\n"));
        self.clip = Some(transfer::Clip { op: loader.store(), base: self.base.clone(), paths, cut });
    }

    /// Where a paste can go: a folder of the open source, not its root on a phone, not a trash.
    fn can_paste(&self, dir: &str) -> bool {
        self.clip.is_some()
            && self.transfer.is_none()
            && self.paste_ask.is_none()
            && !(self.base.starts_with("mtp://") && dir.is_empty())
            && !source::is_trash(dir)
    }

    /// Ctrl+V: into `dir` (`None` = the folder open). First the loader looks for names already there.
    fn paste(&mut self, dir: Option<String>) {
        let dir = dir.unwrap_or_else(|| self.dir.clone());
        if !self.can_paste(&dir) {
            return;
        }
        let (Some(loader), Some(clip)) = (&self.loader, &self.clip) else { return };
        log!("paste into {dir:?}");
        loader.paste_check(clip.clone(), dir.clone());
        self.paste_ask = Some(PasteAsk { dir, conflicts: None, choices: HashMap::new(), all: false });
    }

    fn start_paste(&mut self, dir: String, choices: HashMap<String, transfer::Choice>) {
        let (Some(loader), Some(clip)) = (&self.loader, &self.clip) else { return };
        let progress = Arc::new(transfer::Progress::default());
        loader.paste(clip.clone(), dir.clone(), choices, progress.clone());
        self.transfer = Some((progress, clip.cut, self.base.clone(), dir));
        // One paste per copy (chosen by the user): the clip, and its badges, go.
        self.clip = None;
    }

    /// Asks about each item whose name is already in the destination; then the copy starts.
    fn paste_window(&mut self, ctx: &egui::Context) {
        let Some(ask) = &mut self.paste_ask else { return };
        let Some(conflicts) = &ask.conflicts else { return };
        let Some(path) = conflicts.first().cloned() else {
            let ask = self.paste_ask.take().unwrap();
            self.start_paste(ask.dir, ask.choices);
            return;
        };
        let many = conflicts.len() > 1;
        let t = |id| self.i18n.t(id);
        let text = self.i18n.t_args("paste-taken", &[("name", source::name(&path))]);
        let first = if path.ends_with('/') { t("restore-merge") } else { t("restore-replace") };
        let buttons = [
            (first, transfer::Choice::Replace),
            (t("paste-skip"), transfer::Choice::Skip),
            (t("restore-keep-both"), transfer::Choice::KeepBoth),
        ];
        let (cancel, all_label) = (t("delete-no"), t("paste-all"));
        let mut answer = self.delete_answer.take().and_then(|yes| (!yes).then_some(None));
        egui::Modal::new(egui::Id::unique("paste")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(text);
            if many {
                ui.checkbox(&mut ask.all, all_label);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (label, choice) in buttons {
                    if ui.button(label).clicked() {
                        answer = Some(Some(choice));
                    }
                }
                if ui.button(cancel).clicked() {
                    answer = Some(None);
                }
            });
        });
        match answer {
            Some(Some(choice)) => {
                let conflicts = ask.conflicts.as_mut().unwrap();
                let n = if ask.all { conflicts.len() } else { 1 };
                for p in conflicts.drain(..n) {
                    ask.choices.insert(p, choice);
                }
            }
            Some(None) => self.paste_ask = None,
            None => {}
        }
    }

    /// Progress of the copy, with Cancel; the window stays until the copy ends.
    fn transfer_window(&mut self, ctx: &egui::Context) {
        let Some((p, _, base, _)) = &self.transfer else { return };
        // Folders being filled, read again once a second if they are on screen. Local ones too: the
        // watcher sees only the folder open when the change happens, so a folder listed before and
        // opened during the copy stayed old. After every file it was ten listings a second, slowing the copy down.
        let now = ctx.input(|i| i.time);
        if now - self.touched_at >= 1.0 {
            self.touched_at = now;
            let touched: HashSet<String> = p.touched.lock().unwrap().drain(..).collect();
            if *base == self.base {
                for dir in touched {
                    if self.listings.contains_key(&dir) {
                        self.relist(dir);
                    }
                }
            }
        }
        let Some((p, cut, base, dest)) = &self.transfer else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed));
        let (files, files_total) = (p.files.load(Ordering::Relaxed), p.files_total.load(Ordering::Relaxed));
        let found = p.found.load(Ordering::Relaxed);
        // Until the copy starts it is still looking inside the folders, even with nothing found yet.
        let counts = if p.started.get().is_none() {
            self.i18n.t_args("transfer-looking", &[("n", &found.to_string())])
        } else {
            self.i18n.t_args("transfer-counts", &[
            ("n", &files.to_string()),
            ("total", &files_total.to_string()),
            ("done", &self.human_size(done)),
            ("size", &self.human_size(total)),
        ])
        };
        // Average speed since the copy started; the first second gives no meaningful estimate.
        let left = p.started.get().map(|t| t.elapsed().as_secs_f64()).filter(|&secs| secs >= 1.0 && done > 0).map(|secs| {
            let speed = done as f64 / secs;
            self.i18n.t_args("transfer-left", &[
                ("percent", &(done * 100 / total.max(1)).to_string()),
                ("speed", &self.human_size(speed as u64)),
                ("time", &human_time((total.saturating_sub(done) as f64 / speed) as u64)),
            ])
        });
        let to = self.i18n.t_args("transfer-to", &[("dir", &shown(base, dest))]);
        let current = p.current.lock().unwrap().clone();
        let cancelling = p.cancel.load(Ordering::Relaxed);
        egui::Window::new(self.i18n.t(if *cut { "transfer-moving" } else { "transfer-copying" }))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -40.0))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.add(egui::Label::new(egui::RichText::new(current).strong()).truncate());
                ui.add(egui::Label::new(to).truncate());
                ui.label(counts);
                ui.label(left.unwrap_or_default());
                // Accent fill on a grey track: the default (selection blue on white) hid the empty part.
                ui.scope(|ui| {
                    ui.visuals_mut().extreme_bg_color = ui.visuals().widgets.inactive.bg_fill;
                    let fill = theme::accent(ui.visuals());
                    let fraction = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                    ui.add(egui::ProgressBar::new(fraction).fill(fill).desired_height(16.0));
                });
                if ui.add_enabled(!cancelling, egui::Button::new(self.i18n.t("delete-no"))).clicked() {
                    p.cancel.store(true, Ordering::Relaxed);
                }
            });
    }

    fn ask_delete(&mut self, path: Option<String>, permanent: bool) {
        // Not while a deletion runs: a click repeated while waiting landed on the next item
        // (Download/Pipe, 2026-10-22).
        if self.deleting > 0 {
            return;
        }
        let paths = self.chosen(path);
        let Some(path) = paths.first().cloned() else { return };
        let Some(loader) = &self.loader else { return };
        // In the trash everything goes for good: there is no trash of the trash.
        let how = match permanent || source::is_trash(&path) || source::in_trash(&path) {
            true => source::Deletion::Permanent,
            false => loader.deletion(&path, self.settings.local_permanent, self.settings.remote_delete),
        };
        log!("delete? {paths:?} {how:?}");
        let size = match paths.len() {
            1 => self.listings.get(&source::parent(&path)).and_then(|l| l.as_ref().ok()).and_then(|l| {
                l.files.iter().find(|f| f.path == path).map(|f| f.size)
            }),
            _ => Some(self.images_size(&paths)),
        };
        let ntfs = how == source::Deletion::SystemTrash && !self.base.contains("://") && devices::on_ntfs(&format!("/{}", path.trim_end_matches('/')));
        self.delete_confirm = Some((paths, size, how, ntfs));
    }

    /// Confirmation of the deletion: what will happen, name and size. Enter confirms, Esc cancels.
    fn delete_window(&mut self, ctx: &egui::Context) {
        let Some((paths, size, how, ntfs)) = self.delete_confirm.clone() else { return };
        let path = &paths[0];
        let dir = if path.ends_with('/') { "-dir" } else { "" };
        let id = match how {
            _ if paths.len() > 1 => format!("delete-confirm-many-{}", match how {
                source::Deletion::SystemTrash => "trash",
                source::Deletion::TrashFolder => "folder",
                source::Deletion::Permanent => "permanent",
            }),
            _ if source::is_trash(path) => "delete-confirm-empty-trash".to_string(),
            source::Deletion::SystemTrash => format!("delete-confirm{dir}-trash"),
            source::Deletion::TrashFolder => format!("delete-confirm{dir}-folder"),
            source::Deletion::Permanent => format!("delete-confirm{dir}-permanent"),
        };
        let size = size.map(|s| self.human_size(s)).unwrap_or_default();
        let n = paths.len().to_string();
        let mut text = self.i18n.t_args(&id, &[("name", source::name(path)), ("size", &size), ("n", &n)]);
        if ntfs {
            text = format!("{text}\n\n{}", self.i18n.t("delete-ntfs"));
        }
        let (yes, no) = (self.i18n.t("delete-yes"), self.i18n.t("delete-no"));
        let mut answer = self.delete_answer.take();
        egui::Modal::new(egui::Id::unique("delete")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(yes).clicked() {
                    answer = Some(true);
                }
                if ui.button(no).clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                if let Some(loader) = &self.loader {
                    log!("delete confirmed: {} items", paths.len());
                    self.deleting += paths.len();
                    loader.delete(paths, how);
                }
                self.marked.clear();
                self.delete_confirm = None;
            }
            Some(false) => self.delete_confirm = None,
            None => {}
        }
    }

    fn ask_rename(&mut self, path: Option<String>) {
        let path = path.or_else(|| self.target()).filter(|p| self.can_change(p) && !source::is_trash(p) && !source::in_trash(p));
        if let Some(path) = path {
            log!("rename? {path:?}");
            self.rename = Some((source::name(&path).to_string(), path, true));
        }
    }

    /// New name, in a text field with the old one. Enter confirms, Esc cancels. The name is
    /// checked here: not empty, no separators, not already in the folder.
    fn rename_window(&mut self, ctx: &egui::Context) {
        let Some((mut name, path, first)) = self.rename.clone() else { return };
        let old = source::name(&path).to_string();
        let parent = source::parent(&path);
        let trimmed = name.trim().to_string();
        let taken = self.listings.get(&parent).and_then(|l| l.as_ref().ok()).is_some_and(|l| {
            l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).any(|p| source::name(p) == trimmed)
        });
        let problem = if trimmed.is_empty() || trimmed == "." || trimmed == ".." || trimmed.contains(['/', '\\']) {
            Some(self.i18n.t("rename-bad"))
        } else if trimmed != old && taken {
            Some(self.i18n.t("rename-taken"))
        } else {
            None
        };
        let (title, ok, cancel) = (self.i18n.t("rename-title"), self.i18n.t("rename-ok"), self.i18n.t("delete-no"));
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("rename")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.strong(title);
            ui.add_space(4.0);
            let edit = egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY);
            let mut out = edit.show(ui);
            if first {
                // Focus, and the name selected without the extension, as file managers do.
                out.response.request_focus();
                let end = if path.ends_with('/') { old.chars().count() } else { old.rfind('.').map_or(old.chars().count(), |i| old[..i].chars().count()) };
                let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(end));
                out.state.cursor.set_char_range(Some(range));
                out.state.store(ui.ctx(), out.response.id);
            }
            if let Some(p) = &problem {
                ui.label(browser::error_text(ui, p.as_str()));
            }
            ui.add_space(8.0);
            let enter = out.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                if ui.add_enabled(problem.is_none(), egui::Button::new(ok)).clicked() || (enter && problem.is_none()) {
                    answer = Some(true);
                }
                if ui.button(cancel).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                if trimmed != old
                    && let Some(loader) = &self.loader
                {
                    loader.rename(path, trimmed);
                }
                self.rename = None;
            }
            Some(false) => self.rename = None,
            None => self.rename = Some((name, path, false)),
        }
    }

    /// Item deleted: out of the listing, as if read again. A deleted folder also takes the
    /// listings under it, and the current folder if it was inside. `to_trash`: moved into the
    /// trash folder next to it, whose button appears if it was not there (phones and servers have
    /// no watcher to tell).
    fn remove_item(&mut self, path: &str, to_trash: bool) {
        self.marked.remove(path);
        let dir = source::parent(path);
        let trash = format!("{dir}{}/", source::TRASH_DIR);
        self.listings.retain(|k, _| !(path.ends_with('/') && k.starts_with(path)) && !(to_trash && *k == trash));
        if self.dir.starts_with(path) {
            self.listings.remove(&dir);
            self.set_dir(dir);
            return;
        }
        let Some(Ok(l)) = self.listings.get(&dir) else { return };
        let new = Listing {
            dirs: l.dirs.iter().filter(|d| *d != path).cloned().collect(),
            files: l.files.iter().filter(|f| f.path != path).cloned().collect(),
            trash: if to_trash { Some(trash) } else { l.trash.clone().filter(|t| t != path) },
            leftovers: Vec::new(),
        };
        self.swap_listing(&dir, new);
    }

    /// Item renamed: its folder is read again (its place in the order may change); if the current
    /// folder was inside a renamed one, it follows the new name.
    fn renamed(&mut self, old: &str, new: &str) {
        self.marked.remove(old);
        let dir = source::parent(old);
        if old.ends_with('/') {
            self.listings.retain(|k, _| !k.starts_with(old));
            if self.dir.starts_with(old) {
                let current = format!("{new}{}", &self.dir[old.len()..]);
                self.listings.remove(&dir);
                self.set_dir(current);
                return;
            }
        }
        self.relist(dir);
    }

    /// Reads a folder again in the background; the new listing replaces the old one in place.
    fn relist(&mut self, dir: String) {
        if let Some(loader) = &self.loader
            && self.listing_pending.insert(dir.clone())
        {
            self.relisting.insert(dir.clone());
            loader.list(dir);
        }
    }

    fn ask_restore(&mut self, path: Option<String>) {
        let Some(path) = path.or_else(|| self.target()) else { return };
        log!("restore? {path:?}");
        if source::is_trash(&path) {
            if let Some(loader) = &self.loader {
                loader.trash_items(path);
            }
        } else if source::in_trash(&path) {
            self.restore_queue.push_back(path);
        }
    }

    /// Sends the next item of the queue, when the previous one is done and nothing is being asked.
    fn next_restore(&mut self) {
        if self.restore_busy || self.restore_conflict.is_some() {
            return;
        }
        if let (Some(path), Some(loader)) = (self.restore_queue.pop_front(), &self.loader) {
            self.restore_busy = true;
            loader.restore(path, source::Conflict::Ask);
        }
    }

    /// The name of an item being put back is taken: files can replace or be kept both, folders
    /// can merge or be kept both. Cancel also stops the rest of a Restore all.
    fn restore_window(&mut self, ctx: &egui::Context) {
        let Some(path) = self.restore_conflict.clone() else { return };
        let folder = path.ends_with('/');
        let place = source::name(&source::parent(&source::parent(&path))).to_string();
        let text = self.i18n.t_args("restore-taken", &[("name", source::name(&path)), ("folder", &place)]);
        let t = |id| self.i18n.t(id);
        let first = if folder {
            (t("restore-merge"), source::Conflict::Merge)
        } else {
            (t("restore-replace"), source::Conflict::Replace)
        };
        let buttons = [first, (t("restore-keep-both"), source::Conflict::KeepBoth)];
        let cancel = t("delete-no");
        let mut answer = self.delete_answer.take().and_then(|yes| (!yes).then_some(None));
        egui::Modal::new(egui::Id::unique("restore")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (label, conflict) in buttons {
                    if ui.button(label).clicked() {
                        answer = Some(Some(conflict));
                    }
                }
                if ui.button(cancel).clicked() {
                    answer = Some(None);
                }
            });
        });
        match answer {
            Some(Some(conflict)) => {
                if let Some(loader) = &self.loader {
                    self.restore_busy = true;
                    loader.restore(path, conflict);
                }
                self.restore_conflict = None;
            }
            Some(None) => {
                self.restore_conflict = None;
                self.restore_queue.clear();
            }
            None => {}
        }
    }

    /// New listing of a folder already shown (image deleted, folder changed on disk). Thumbnails
    /// and viewer images are kept by position: they follow their file to its new place, so the
    /// next image in the viewer is already there; those of files gone or changed are dropped.
    /// Selection and viewer follow their item; if it is gone, what took its place.
    /// Requests in flight carry the old positions: a new generation drops them and they are asked again.
    fn swap_listing(&mut self, dir: &str, new: Listing) {
        let Some(Ok(old)) = self.listings.insert(dir.to_string(), Ok(new)) else { return };
        if dir != self.dir {
            return;
        }
        let Some(Ok(new)) = self.listings.get(dir) else { return };
        let at: HashMap<&str, usize> = new.files.iter().enumerate().map(|(i, f)| (f.path.as_str(), i)).collect();
        let moved: HashMap<usize, usize> = old
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let j = *at.get(f.path.as_str())?;
                (new.files[j].size == f.size && new.files[j].modified == f.modified).then_some((i, j))
            })
            .collect();
        fn remap<T>(map: &mut HashMap<usize, T>, moved: &HashMap<usize, usize>) {
            *map = map.drain().filter_map(|(k, v)| Some((*moved.get(&k)?, v))).collect();
        }
        remap(&mut self.thumbs, &moved);
        remap(&mut self.full, &moved);
        self.generated = self.generated.iter().filter_map(|k| moved.get(k).copied()).collect();
        let (ndirs, n) = (new.dirs.len(), new.files.len());
        let file_at = |i: usize| old.files.get(i).and_then(|f| at.get(f.path.as_str()).copied());
        let followed = match old.dirs.get(self.selected) {
            Some(d) => new.dirs.iter().position(|x| x == d),
            None => file_at(self.selected - old.dirs.len()).map(|j| ndirs + j),
        };
        self.selected = followed.unwrap_or(self.selected).min((ndirs + n).saturating_sub(1));
        self.cursor &= ndirs + n > 0;
        self.viewing = self.viewing.filter(|_| n > 0).map(|v| file_at(v).unwrap_or(v.min(n - 1)));
        if let Some(loader) = &self.loader {
            self.generation = loader.next_generation();
        }
        self.thumb_queue_key = None;
        self.pending.clear();
        self.meta_asked = None;
    }

    fn close_viewer(&mut self) {
        if let (Some(i), Some(Ok(l))) = (self.viewing.take(), self.listings.get(&self.dir)) {
            self.selected = l.dirs.len() + i;
            self.cursor = true;
            self.scroll_to_selected = true;
        }
        self.full.clear();
        self.zoom_of = None;
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
        let t = |id| self.i18n.t(id);
        let item = |label: String, keys: String| egui::Button::new(label).shortcut_text(keys);
        let viewing = self.viewing.is_some();
        let has_items = self.items() > 0;
        let can_go_up = !viewing && !self.dir.is_empty();
        // A Cell: the plain items, the radio buttons and the check boxes all record into it.
        let act = std::cell::Cell::new(None);
        let pick = |r: egui::Response, a: Act| {
            if r.clicked() {
                act.set(Some(a));
            }
        };
        egui::MenuBar::new().ui(ui, |ui| {
            let entry = |ui: &mut egui::Ui, enabled: bool, button: egui::Button, a: Act| pick(ui.add_enabled(enabled, button), a);
            ui.menu_button(t("menu-file"), |ui| {
                entry(ui, true, egui::Button::new(t("menu-open-location")), Act::Address);
                entry(ui, true, egui::Button::new(t("menu-options")), Act::Options);
                ui.separator();
                let can_clip = !self.chosen(None).is_empty();
                entry(ui, can_clip, item(t("menu-cut"), t("key-cut")), Act::Cut(None));
                entry(ui, can_clip, item(t("menu-copy"), t("key-copy")), Act::Copy(None));
                entry(ui, !viewing && self.can_paste(&self.dir), item(t("menu-paste"), t("key-paste")), Act::Paste(None));
                ui.separator();
                entry(ui, self.can_save(), item(t("menu-save"), t("key-save")), Act::Save);
                entry(ui, viewing, item(t("menu-save-copy"), t("key-save-copy")), Act::Convert(None));
                ui.separator();
                let can_change = self.target().is_some();
                entry(ui, can_change, item(t("menu-rename"), "F2".into()), Act::Rename(None));
                entry(ui, can_change, item(t("menu-delete"), t("key-delete")), Act::Delete(None, false));
                entry(ui, can_change, item(t("menu-delete-permanent"), t("key-shift-delete")), Act::Delete(None, true));
                ui.separator();
                entry(ui, true, egui::Button::new(t("menu-quit")), Act::Quit);
            });
            ui.menu_button(t("menu-image"), |ui| {
                ui.add_enabled_ui(viewing || !self.grid_images(None).is_empty(), |ui| {
                    ui.menu_button(t("menu-transform"), |ui| {
                        for (op, label, keys) in transform_items() {
                            entry(ui, true, item(t(label), keys.map(t).unwrap_or_default()), Act::Transform(op, None));
                            if op == transform::Op::FlipV {
                                ui.separator();
                            }
                        }
                    });
                });
            });
            ui.menu_button(t("menu-view"), |ui| {
                entry(ui, !viewing && has_items && self.cursor, item(t("menu-open-selected"), t("key-enter")), Act::Open);
                entry(ui, viewing, item(t("menu-back-to-grid"), "Esc".into()), Act::Back);
                ui.separator();
                entry(ui, true, item(t("menu-refresh"), "F5".into()), Act::Refresh);
                entry(ui, self.chosen(None).iter().any(|p| !p.ends_with('/')), egui::Button::new(t("menu-redo-thumbs")), Act::RedoThumbs(None));
                entry(ui, !viewing && has_items, item(t("menu-redo-all-thumbs"), t("key-shift-f5")), Act::RedoAllThumbs);
                ui.separator();
                ui.menu_button(t("sort-by"), |ui| {
                    for by in settings::SortBy::ALL {
                        pick(ui.radio(self.settings.sort_by == by, t(by.label())), Act::Sort(by));
                    }
                    ui.separator();
                    pick(ui.checkbox(&mut self.settings.sort_desc.clone(), t("sort-desc")), Act::SortDesc);
                });
                ui.add_enabled_ui(!viewing, |ui| {
                    ui.menu_button(t("menu-meta"), |ui| {
                        for g in meta::Group::ALL {
                            let label = self.i18n.t(&format!("meta-{}", g.key()));
                            pick(ui.checkbox(&mut self.meta_shown(g), label), Act::Meta(g));
                        }
                    });
                });
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
                ui.separator();
                entry(ui, true, egui::Button::new(t("menu-check-updates")), Act::CheckUpdates);
                entry(ui, true, egui::Button::new(t("menu-about")), Act::About);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("EyeCanSee {}", version()));
                if let Some(r) = &self.update {
                    // Red with white text (contrast 5.6:1): as a plain button it went unnoticed.
                    let text = egui::RichText::new(self.i18n.t_args("update-install", &[("version", &r.version)])).color(Color32::WHITE).strong();
                    pick(ui.add(egui::Button::new(text).fill(Color32::from_rgb(0xC6, 0x28, 0x28))), Act::Update);
                }
            });
        });
        if let Some(a) = act.take() {
            self.run(ui.ctx(), a);
        }
    }

    /// Runs an item chosen from the menu bar or from a right-click menu.
    pub(crate) fn run(&mut self, ctx: &egui::Context, act: Act) {
        log!("menu {act:?}");
        match act {
            Act::Address => self.open_address = Some(AddressForm { first: true, ..Default::default() }),
            Act::Options => self.options = true,
            Act::Delete(path, permanent) => self.ask_delete(path, permanent),
            Act::Rename(path) => self.ask_rename(path),
            Act::Convert(path) => self.open_convert(path),
            Act::RedoThumbs(path) => self.redo_thumbs(ctx, self.chosen(path)),
            Act::RedoAllThumbs => self.redo_all_thumbs(ctx),
            Act::Transform(op, path) => {
                if self.viewing.is_some() {
                    self.turn = self.turn.then(op);
                } else {
                    let paths = self.grid_images(path);
                    if !paths.is_empty() {
                        self.batch_turn = Some((paths, op));
                    }
                }
            }
            Act::Save => self.ask_save(),
            Act::Submenu(_) => {}
            Act::Restore(path) => self.ask_restore(path),
            Act::Copy(path) => self.clip(ctx, path, false),
            Act::Cut(path) => self.clip(ctx, path, true),
            Act::Paste(dir) => self.paste(dir),
            Act::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Act::Update => {
                let Some(ready) = &self.update else { return };
                match update::apply(ready) {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => {
                        log!("update: ERROR {e}");
                        self.error = Some(self.i18n.t_args("update-failed", &[("detail", &e)]));
                    }
                }
            }
            Act::Open => self.activate(self.selected),
            Act::OpenDir(dir) => self.set_dir(dir),
            Act::Back => self.close_viewer(),
            Act::Refresh => self.refresh(),
            Act::Go(to) => self.go(to),
            Act::Up => self.go_up(),
            Act::Manual => self.help = true,
            Act::About => self.about = true,
            Act::CheckUpdates => {
                let id = if let Some(r) = &self.update {
                    self.update_note = Some(self.i18n.t_args("update-found", &[("version", &r.version)]));
                    return;
                } else if self.check_updates(ctx) {
                    "update-checking"
                } else {
                    "update-unavailable"
                };
                self.update_note = Some(self.i18n.t(id));
            }
            Act::Meta(g) => self.show_meta(g, !self.meta_shown(g)),
            Act::Sort(by) => self.set_sort(by, self.settings.sort_desc),
            Act::SortDesc => self.set_sort(self.settings.sort_by, !self.settings.sort_desc),
        }
    }

    /// In the background; the answer comes as `Msg::Update`. `false` = this copy is not updated
    /// by EyeCanSee (RPM, development build).
    fn check_updates(&self, ctx: &egui::Context) -> bool {
        let Some(kind) = update::Kind::detect() else { return false };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            if tx.send(Msg::Update(update::check(kind, version()))).is_ok() {
                ctx.request_repaint();
            }
        });
        true
    }

    /// Help > Check for updates: the outcome, until closed.
    fn update_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &self.update_note else { return };
        let mut open = true;
        egui::Window::new(self.i18n.t("menu-check-updates"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.label(text);
            });
        if !open {
            self.update_note = None;
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.about;
        let icon = self.about_icon.get_or_insert_with(|| {
            let img = decode::app_icon(96).unwrap_or_default();
            ctx.load_texture("about-icon", decode::to_color_image(img.into()), TextureOptions::LINEAR)
        });
        let t = |id| self.i18n.t(id);
        egui::Window::new(t("menu-about"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.image((icon.id(), egui::vec2(96.0, 96.0)));
                    ui.heading(format!("EyeCanSee {}", version()));
                    ui.label(t("about-tagline"));
                    ui.add_space(8.0);
                    ui.label("© 2026 Pierluigi Montinaro");
                    ui.label(t("about-license"));
                    ui.hyperlink_to(t("about-license-link"), "https://www.gnu.org/licenses/gpl-3.0.html");
                    ui.hyperlink_to(t("about-source"), env!("CARGO_PKG_REPOSITORY"));
                    ui.add_space(8.0);
                    ui.weak(t("about-third-party"));
                });
            });
        self.about = open;
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Coloured icons in square buttons with a border (style "B").
        let icon = |icon| egui::Button::image(icons::image(ctx, icon, 18.0)).min_size(egui::vec2(32.0, 28.0));
        ui.horizontal(|ui| {
            if self.viewing.is_some() {
                if ui.add(icon(icons::Icon::Grid)).on_hover_text(self.i18n.t("menu-back-to-grid")).clicked() {
                    self.close_viewer();
                }
            } else if ui
                .add_enabled(!self.dir.is_empty(), icon(icons::Icon::UpButton))
                .on_hover_text(self.i18n.t("tb-up"))
                .clicked()
            {
                self.go_up();
            }
            if ui.add(icon(icons::Icon::Refresh)).on_hover_text(self.i18n.t("menu-refresh")).clicked() {
                self.refresh();
            }
            if self.viewing.is_some() {
                for (i, op, id) in [(icons::Icon::RotateCcw, transform::Op::Ccw, "tf-rotate-ccw"), (icons::Icon::RotateCw, transform::Op::Cw, "tf-rotate-cw")] {
                    if ui.add(icon(i)).on_hover_text(self.i18n.t(id)).clicked() {
                        self.act = Some(Act::Transform(op, None));
                    }
                }
            }
            // Metadata: closes all the boxes if one is open, otherwise reopens them all.
            if self.viewing.is_none() {
                let any = meta::Group::ALL.into_iter().any(|g| self.meta_shown(g));
                let button = egui::Button::selectable(any, icons::image(ctx, icons::Icon::Info, 18.0)).min_size(egui::vec2(32.0, 28.0));
                if ui.add(button).on_hover_text(self.i18n.t("tb-meta")).clicked() {
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
        self.meta_height = 0.0;
        self.settings.save();
    }

    /// How many folders and images there are, and name and size of the selected item.
    /// On the solid accent colour (style "B"), all in white.
    fn status_bar(&self, ui: &mut egui::Ui) {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        ui.horizontal(|ui| {
            if self.deleting > 0 {
                ui.spinner();
                ui.label(self.i18n.t_args("status-deleting", &[("n", &self.deleting.to_string())]));
                ui.separator();
            }
            if !self.saving.is_empty() {
                ui.spinner();
                ui.label(self.i18n.t_args("status-saving-n", &[("n", &self.saving.len().to_string())]));
                ui.separator();
            }
            if let Some(clip) = &self.clip {
                let (icon, id) = if clip.cut { (icons::Icon::Cut, "status-cut") } else { (icons::Icon::Copied, "status-copied") };
                ui.add(icons::image(ui.ctx(), icon, 16.0));
                ui.label(self.i18n.t_args(id, &[("n", &clip.paths.len().to_string())]));
                ui.separator();
            }
            ui.label(self.i18n.t_args("status-folders", &[("n", &l.dirs.len().to_string())]));
            ui.label(self.i18n.t_args("status-images", &[("n", &l.files.len().to_string())]));
            let marked = self.marked_paths();
            if !marked.is_empty() {
                ui.separator();
                let size = self.human_size(self.images_size(&marked));
                ui.label(self.i18n.t_args("status-marked", &[("n", &marked.len().to_string()), ("size", &size)]));
                return;
            }
            if !self.cursor {
                return;
            }
            ui.separator();
            match self.selected.checked_sub(l.dirs.len()).and_then(|i| l.files.get(i)) {
                Some(f) => {
                    ui.label(source::name(&f.path));
                    ui.label(self.human_size(f.size));
                    if let Some(date) = self.file_date(&f.modified) {
                        ui.label(date);
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
        let mut chosen = None;
        if let Some(Ok(tex)) = self.full.get(&current) {
            let tex = tex.clone();
            let rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            self.zoom_view(ui, &response, &tex, current);
            if response.double_clicked() {
                self.close_viewer();
            }
            response.context_menu(|ui| {
                chosen = browser::context_menu(ui, self.item_menu(None, None));
            });
            if chosen.is_some() {
                self.act = chosen;
            }
            return;
        }
        ui.centered_and_justified(|ui| match self.full.get(&current) {
            Some(Ok(_)) => {}
            Some(Err(e)) => {
                let name = self.files().get(current).map_or("", |f| source::name(&f.path));
                ui.label(browser::error_text(ui, self.i18n.t_args("cannot-load", &[("file", name), ("error", e)])));
            }
            None => {
                ui.spinner();
            }
        });
        if chosen.is_some() {
            self.act = chosen;
        }
    }

    /// The open image, fitted to the window or zoomed (as GNOME's viewer, Loupe): the wheel ×1.3 a
    /// notch around the pointer (or the next image, see `wheel_zoom`; Ctrl and the wheel always
    /// zoom), + and − ×1.5 around the centre, 0 fitted, 1 at 100%, dragging moves the zoomed image.
    /// Never smaller than fitted, never larger than `max_zoom`.
    fn zoom_view(&mut self, ui: &egui::Ui, response: &egui::Response, tex: &TextureHandle, current: usize) {
        let rect = response.rect;
        let ppp = ui.ctx().pixels_per_point();
        if self.zoom_of != Some(current) {
            // ponytail: changes not saved are dropped without asking, they are only turns
            (self.zoom, self.zoom_of, self.turn) = (None, Some(current), Default::default());
        }
        let t = convert::turned_size(tex, self.turn);
        let fit = (rect.width() * ppp / t.x).min(rect.height() * ppp / t.y).min(1.0);
        let max = (self.settings.max_zoom as f32 / 100.0).max(fit);
        let scale = self.zoom.unwrap_or(fit).clamp(fit, max);

        let (mut wheel, mut next) = (0.0, 0.0);
        let mut pinch = 1.0;
        if response.hovered() {
            let wheel_zoom = self.settings.wheel_zoom;
            ui.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::MouseWheel { unit, delta, modifiers, .. } => {
                            // Notches: a mouse gives lines, a touchpad points.
                            let notches = match unit {
                                egui::MouseWheelUnit::Line => delta.y,
                                egui::MouseWheelUnit::Point => delta.y / 50.0,
                                egui::MouseWheelUnit::Page => delta.y * 3.0,
                            };
                            if wheel_zoom || modifiers.command {
                                wheel += notches;
                            } else {
                                next -= notches;
                            }
                        }
                        egui::Event::Zoom(f) => pinch *= f,
                        _ => {}
                    }
                }
            });
        }
        self.wheel_notches += next;
        while self.wheel_notches.abs() >= 1.0 {
            let forward = self.wheel_notches > 0.0;
            self.wheel_notches -= self.wheel_notches.signum();
            self.go(if forward { Go::Next } else { Go::Prev });
        }
        let (zoom_in, zoom_out, fitted, actual) = if ui.ctx().egui_wants_keyboard_input() {
            (false, false, false, false)
        } else {
            ui.input(|i| {
                let key = |k| i.key_pressed(k) && !i.modifiers.command;
                (key(Key::Plus) || key(Key::Equals), key(Key::Minus), key(Key::Num0), key(Key::Num1))
            })
        };

        let mut target = scale * 1.3f32.powf(wheel) * pinch;
        let mut anchor = response.hover_pos().filter(|_| wheel != 0.0 || pinch != 1.0).unwrap_or(rect.center());
        if zoom_in || zoom_out {
            target = scale * if zoom_in { 1.5 } else { 1.0 / 1.5 };
            anchor = rect.center();
        }
        if actual {
            target = 1.0;
        }
        let target = target.clamp(fit, max);
        if fitted || target <= fit {
            (self.zoom, self.pan) = (None, t / 2.0);
        } else if target != scale {
            // The image point under the anchor stays under it.
            let from_centre = (anchor - rect.center()) * ppp;
            let base = if self.zoom.is_some() { self.pan } else { t / 2.0 };
            self.pan = base + from_centre / scale - from_centre / target;
            self.zoom = Some(target);
        }
        if self.zoom.is_some() && response.dragged() {
            self.pan -= response.drag_delta() * ppp / self.zoom.unwrap_or(fit);
        }

        let painter = ui.painter_at(rect);
        match self.zoom {
            Some(scale) => {
                if response.hovered() {
                    ui.ctx().set_cursor_icon(if response.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                }
                convert::paint_zoomed(&painter, rect, tex, scale, &mut self.pan, self.turn);
            }
            None => {
                let dest = egui::Rect::from_center_size(rect.center(), t * fit / ppp);
                convert::paint_turned(&painter, tex, dest, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), self.turn);
            }
        }
        self.zoom_shown = self.zoom.unwrap_or(fit);
    }
}

impl App {
    /// A confirmation is open: deletion, or a name already taken when restoring or pasting.
    pub(crate) fn asking(&self) -> bool {
        let pasting = self.paste_ask.as_ref().is_some_and(|a| a.conflicts.as_ref().is_some_and(|c| !c.is_empty()));
        self.delete_confirm.is_some() || self.restore_conflict.is_some() || pasting || self.save_ready.is_some() || self.batch_turn.is_some() || self.notice.is_some() || self.open_address.is_some()
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Confirmation open: the keys answer it and reach nothing else (Esc would close the viewer).
        // Open location has fields to type in: there the keys go to the field.
        if self.asking() && !(self.open_address.is_some() && ctx.egui_wants_keyboard_input()) {
            let answer = &mut self.delete_answer;
            raw_input.events.retain(|e| match e {
                egui::Event::Key { key, pressed: true, .. } => {
                    match key {
                        Key::Enter => *answer = Some(true),
                        Key::Escape => *answer = Some(false),
                        _ => {}
                    }
                    false
                }
                egui::Event::Key { .. } | egui::Event::Text(_) => false,
                _ => true,
            });
            return;
        }
        if ctx.egui_wants_keyboard_input() || self.rename.is_some() {
            return;
        }
        // Copy, cut and paste reach the app as events, not keys. On Windows Shift+Del is a cut
        // for egui: here it is the deletion for good, as everywhere else in EyeCanSee.
        let mods = ctx.input(|i| i.modifiers);
        let shift_del = cfg!(windows) && mods.shift && !mods.command;
        let act = &mut self.act;
        raw_input.events.retain(|e| {
            *act = match e {
                egui::Event::Copy => Some(Act::Copy(None)),
                egui::Event::Cut if shift_del => Some(Act::Delete(None, true)),
                egui::Event::Cut => Some(Act::Cut(None)),
                egui::Event::Paste(_) => Some(Act::Paste(None)),
                _ => return true,
            };
            false
        });
        if self.viewing.is_some() {
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
        // Closing during a copy: it is cancelled, and the window closes once the file being copied
        // is finished. Closed in the middle, a phone kept half an operation and the next session
        // got its leftovers ("Transaction ID mismatch", until it was plugged in again).
        if let Some((p, ..)) = &self.transfer
            && ctx.input(|i| i.viewport().close_requested())
        {
            log!("close during a copy: cancelling it first");
            p.cancel.store(true, Ordering::Relaxed);
            self.quit_after_transfer = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.receive(&ctx);
        self.retry(&ctx);
        self.reread_changed(&ctx);
        // "Save in another format" covers the window: the keys must not reach the grid under it.
        if self.convert.is_none() {
            self.global_keys(&ctx);
            if self.viewing.is_some() {
                self.viewer_keys(&ctx);
            }
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

        if self.convert.is_some() {
            egui::CentralPanel::default().show(ui, |ui| self.convert_page(ui));
        } else {
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
                    let bar = egui::Frame::side_top_panel(ui.style()).fill(theme::solid(ui.visuals()));
                    egui::Panel::bottom("viewer-status").frame(bar).show(ui, |ui| self.viewer_status(ui, current));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::central_panel(ui.style()).fill(Color32::from_gray(18)))
                        .show(ui, |ui| self.viewer(ui, current));
                }
                None => {
                    let bar = egui::Frame::side_top_panel(ui.style()).fill(theme::solid(ui.visuals()));
                    egui::Panel::bottom("status").frame(bar).show(ui, |ui| self.status_bar(ui));
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
        self.about_window(&ctx);
        self.update_window(&ctx);
        self.options_window(&ctx);
        if let Some(a) = self.act.take() {
            self.run(&ctx, a);
        }
        self.delete_window(&ctx);
        self.save_window(&ctx);
        self.batch_turn_window(&ctx);
        self.notice_window(&ctx);
        self.address_window(&ctx);
        self.paste_window(&ctx);
        self.transfer_window(&ctx);
        self.rename_window(&ctx);
        self.restore_window(&ctx);
        self.next_restore();

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
    // A local path is made absolute: the instance it is handed to has another working folder.
    let arg = std::env::args().nth(1).map(|a| match a.contains("://") {
        true => a,
        false => std::path::absolute(&a).map_or(a, |p| p.to_string_lossy().into_owned()),
    });
    // The screenshot of a test run must not end up in the user's window.
    if std::env::var_os("EYECANSEE_SCREENSHOT").is_none() && single::hand_over(arg.as_deref().unwrap_or("")) {
        return Ok(());
    }
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

/// A folder of a source as the address bar shows it: `base` plus the folder, with `\` on Windows.
fn shown(base: &str, dir: &str) -> String {
    let sep = if base.contains("://") || !base.contains('\\') { "/" } else { "\\" };
    format!("{base}{}", dir.replace('/', sep))
}

/// "40 s", "1 min 15 s", "2 h 5 min": the time left of a copy.
fn human_time(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{} min {} s", secs / 60, secs % 60),
        _ => format!("{} h {} min", secs / 3600, secs % 3600 / 60),
    }
}

/// Protocols of File > Open location (SFTP only where OpenDAL has it).
const SCHEMES: &[&str] = if cfg!(unix) { &["ftp", "ftps", "sftp"] } else { &["ftp", "ftps"] };

/// File > Open location: a whole address, or its parts.
#[derive(Clone, Default)]
struct AddressForm {
    url: String,
    scheme: usize,
    host: String,
    port: String,
    user: String,
    password: String,
    folder: String,
    /// Just opened: the address field takes the focus.
    first: bool,
}

impl AddressForm {
    /// The address to open: the one typed whole, otherwise built from the parts (user and
    /// password escaped). `None`: nothing to open yet, or a port that is not a number.
    fn address(&self) -> Option<String> {
        if !self.url.trim().is_empty() {
            return Some(self.url.trim().to_string());
        }
        let host = self.host.trim();
        if host.is_empty() {
            return None;
        }
        let mut u = url::Url::parse(&format!("{}://{host}/", SCHEMES[self.scheme])).ok()?;
        if !self.port.trim().is_empty() {
            u.set_port(Some(self.port.trim().parse().ok()?)).ok()?;
        }
        if !self.user.is_empty() {
            u.set_username(&self.user).ok()?;
        }
        if !self.password.is_empty() {
            u.set_password(Some(&self.password)).ok()?;
        }
        // A folder: with "/" at the end, or its last part would be taken for a file.
        let folder = self.folder.trim().trim_matches('/');
        u.set_path(&if folder.is_empty() { "/".to_string() } else { format!("/{folder}/") });
        Some(u.to_string())
    }
}

impl App {
    /// File > Open location: an address typed whole, or protocol, server, port, user, password
    /// and folder in their own fields.
    fn address_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.open_address.clone() else { return };
        let t = |id| self.i18n.t(id);
        let address = f.address();
        let bad_port = f.url.trim().is_empty() && !f.port.trim().is_empty() && f.port.trim().parse::<u16>().is_err();
        // In a field Enter and Esc reach the window; elsewhere they arrive as `delete_answer`.
        let keys = self.delete_answer.take();
        let mut answer = ctx.input(|i| if i.key_pressed(Key::Escape) || keys == Some(false) { Some(false) } else { None });
        egui::Modal::new(egui::Id::unique("open-address")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.strong(t("addr-title"));
            ui.add_space(4.0);
            let url = ui.add(egui::TextEdit::singleline(&mut f.url).hint_text(t("tb-address-hint")).desired_width(f32::INFINITY));
            if f.first {
                url.request_focus();
                f.first = false;
            }
            ui.add_space(8.0);
            ui.label(t("addr-or"));
            ui.add_space(4.0);
            let parts = f.url.trim().is_empty();
            ui.add_enabled_ui(parts, |ui| {
                egui::Grid::new("addr-parts").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(t("addr-protocol"));
                    egui::ComboBox::from_id_salt("addr-scheme").selected_text(SCHEMES[f.scheme].to_uppercase()).show_ui(ui, |ui| {
                        for (i, s) in SCHEMES.iter().enumerate() {
                            ui.selectable_value(&mut f.scheme, i, s.to_uppercase());
                        }
                    });
                    ui.end_row();
                    let field = |ui: &mut egui::Ui, label: String, text: &mut String, hint: String, password: bool| {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(text).hint_text(hint).password(password).desired_width(300.0));
                        ui.end_row();
                    };
                    field(ui, t("addr-host"), &mut f.host, String::new(), false);
                    let default_port = if SCHEMES[f.scheme] == "sftp" { "22" } else { "21" };
                    field(ui, t("addr-port"), &mut f.port, default_port.to_string(), false);
                    field(ui, t("addr-user"), &mut f.user, t("addr-user-hint"), false);
                    field(ui, t("addr-password"), &mut f.password, String::new(), true);
                    field(ui, t("addr-folder"), &mut f.folder, "/".to_string(), false);
                });
            });
            if bad_port {
                ui.label(browser::error_text(ui, t("addr-bad-port")));
            }
            ui.add_space(8.0);
            let enter = keys == Some(true) || ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                if ui.add_enabled(address.is_some(), egui::Button::new(t("addr-open"))).clicked() || (enter && address.is_some()) {
                    answer = Some(true);
                }
                if ui.button(t("delete-no")).clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                self.open_address = None;
                if let Some(a) = address {
                    self.open_source(ctx, &a);
                }
            }
            Some(false) => self.open_address = None,
            None => self.open_address = Some(f),
        }
    }
}

/// The address for the log: a password becomes "***".
fn without_password(address: &str) -> String {
    match url::Url::parse(address.trim()) {
        Ok(mut u) if u.password().is_some() => {
            let _ = u.set_password(Some("***"));
            u.to_string()
        }
        _ => address.to_string(),
    }
}

/// Font family of the bold text in message windows (`notice_text`).
const BOLD: &str = "bold";

/// egui's fonts plus Ubuntu Bold, the bold of its Ubuntu Light (same licence), for what must
/// stand out in a message: egui has no bold of its own, `strong` only darkens the colour.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(BOLD.into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Ubuntu-Bold.ttf"))));
    let mut family = vec![BOLD.to_string()];
    family.extend(fonts.families[&egui::FontFamily::Proportional].iter().cloned());
    fonts.families.insert(egui::FontFamily::Name(BOLD.into()), family);
    fonts
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_location_address() {
        let f = |host: &str, port: &str, user: &str, password: &str, folder: &str| {
            super::AddressForm { host: host.into(), port: port.into(), user: user.into(), password: password.into(), folder: folder.into(), ..Default::default() }.address()
        };
        assert_eq!(f("", "", "", "", ""), None);
        assert_eq!(f("nas.lan", "", "", "", ""), Some("ftp://nas.lan/".into()));
        assert_eq!(f("127.0.0.1", "2121", "", "", "/Foto/2026/"), Some("ftp://127.0.0.1:2121/Foto/2026/".into()));
        assert_eq!(f("nas.lan", "", "me@home", "p:w/d", "Foto"), Some("ftp://me%40home:p%3Aw%2Fd@nas.lan/Foto/".into()));
        assert_eq!(f("nas.lan", "porta", "", "", ""), None);
        let whole = super::AddressForm { url: " C:\\Foto ".into(), host: "ignored".into(), ..Default::default() };
        assert_eq!(whole.address(), Some("C:\\Foto".into()));
    }

    #[test]
    fn password_out_of_log() {
        assert_eq!(super::without_password("ftp://me:secret@nas.lan/Foto/"), "ftp://me:***@nas.lan/Foto/");
        assert_eq!(super::without_password("ftp://nas.lan/"), "ftp://nas.lan/");
        assert_eq!(super::without_password("C:\\Foto"), "C:\\Foto");
    }


    #[test]
    fn human_times() {
        assert_eq!(super::human_time(40), "40 s");
        assert_eq!(super::human_time(75), "1 min 15 s");
        assert_eq!(super::human_time(7500), "2 h 5 min");
    }

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
