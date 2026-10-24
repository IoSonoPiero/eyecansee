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

mod address;
mod app_dialogs;
mod browser;
mod confirm;
mod convert;
mod decode;
mod devices;
mod file_dialogs;
mod i18n;
mod icons;
mod loader;
mod menu;
mod messages;
mod meta;
mod mtp;
mod navigation;
mod save_dialogs;
mod settings;
mod single;
mod source;
mod theme;
mod thumbcache;
mod transfer;
mod transform;
mod update;
mod viewer;
mod watch;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Key, TextureHandle};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use address::AddressForm;
use i18n::I18n;
use loader::{Loader, Msg};
use settings::{Settings, WindowStart};
use navigation::Go;
use source::Listing;

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
    #[allow(clippy::too_many_lines)] // to be split
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
        app.watcher = watch::folder_watcher(app.tx.clone(), ctx.clone());
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
            // An FTP server with a user: the password is never saved, so instead of a login sure to
            // be refused File > Open location asks for it, with the rest filled in; home behind it.
            (None, Some(last)) if let Some(form) = AddressForm::asking_password(&last) => {
                app.open_address = Some(form);
                app.open_source(ctx, &home);
            }
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

    /// The images (not folders) an action on `path` in the grid works on, as `chosen`.
    fn grid_images(&self, path: Option<String>) -> Vec<String> {
        if self.viewing.is_some() {
            return Vec::new();
        }
        self.chosen(path).into_iter().filter(|p| !p.ends_with('/') && !source::in_trash(p)).collect()
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

    #[allow(clippy::too_many_lines)] // to be split
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

/// "40 s", "1 min 15 s", "2 h 5 min": the time left of a copy.
fn human_time(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{} min {} s", secs / 60, secs % 60),
        _ => format!("{} h {} min", secs / 3600, secs % 3600 / 60),
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
