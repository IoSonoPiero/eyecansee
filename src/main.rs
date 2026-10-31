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

mod actions;
mod address;
mod audio_job;
mod app_dialogs;
mod batch;
mod batch_page;
mod browser;
mod component;
mod confirm;
mod convert;
#[cfg(debug_assertions)]
mod debug_shot;
mod decode;
mod devices;
mod dup_page;
mod duplicates;
mod encode;
mod engine;
mod ffmpeg;
mod file_dialogs;
mod folder_pick;
mod ftp;
mod gps_track;
mod grid;
mod human;
mod i18n;
mod icons;
mod listing;
mod loader;
mod menu;
mod messages;
mod meta;
mod meta_panel;
mod mp4;
mod mpv;
mod mpv_load;
mod mtp;
mod navigation;
mod new_folder;
mod paint;
mod psd;
mod rotate;
mod rotate_page;
mod save_dialogs;
mod selection;
mod server_manager;
mod servers;
mod settings;
mod sftp;
mod single;
mod slideshow;
mod source;
mod theme;
mod thumbcache;
mod transfer;
mod transform;
mod trash;
mod tree;
mod ultrahdr;
mod update;
mod vault_page;
mod video;
mod video_read;
mod viewer;
mod watch;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, Key, TextureHandle};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use actions::Act;
use address::AddressForm;
use i18n::I18n;
use loader::{Loader, Msg};
use settings::{Settings, WindowStart};
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
    Slideshow,
    Video,
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
    /// The video playing in the viewer.
    video: Option<video::Playing>,
    /// The video engine being downloaded (`component::VIDEO`).
    download: Option<std::sync::Arc<component::Download>>,
    /// Why EyeCanSee could not be started again after the engine download.
    restart_error: Option<String>,
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
    /// The image whose "Save" is being prepared, before the confirmation (status bar).
    preparing: Option<String>,
    /// Names left out of a "Save" on the grid (animated…), shown once every save is done.
    skipped: Vec<String>,
    /// Images saved by transforms on the grid, for the summary once every save is done.
    batch_saved: Option<usize>,
    /// An image just saved from Rotate and crop, as the viewer shows it: kept in the viewer when
    /// the file and the folder listing change, instead of the old one until it is read again.
    fresh: Option<(String, TextureHandle)>,
    /// Leaving an image with changes not saved: what was asked, while the question is open.
    after_turn: Option<save_dialogs::After>,
    /// The same, after Save was chosen: done once the save is confirmed.
    after_save: Option<save_dialogs::After>,
    /// A message in a window (a "Save" refused, how a grid save went); `true` offers "Save as…".
    notice: Option<(String, Option<Act>)>,
    /// KDE holds the phone: asking whether to terminate kiod (`kiod_window`).
    kiod_ask: bool,
    /// An SFTP server never used from this computer: asking whether to trust its key (`host_window`).
    host_ask: bool,
    /// In the status bar until the next source: the phone is open through the desktop's mount.
    status_note: Option<String>,
    /// When a passing `status_note` goes away by itself (`egui` time), such as a file removed.
    note_until: Option<f64>,
    /// The message of a stalled phone, while it lasts (`App::phone_stall`).
    stall_notice: Option<String>,
    /// File > Server manager: the window, with the server being changed.
    server_manager: Option<AddressForm>,
    /// The master password asked before a saved server is used.
    unlock: Option<vault_page::Unlock>,
    /// The saved servers, listed in the left panel.
    saved_servers: Vec<servers::Saved>,
    /// The folder last open in each source (by base), to go back to it from the left panel.
    last_dirs: HashMap<String, String>,
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
    /// Rotate and crop, a page of its own.
    rotate: Option<rotate_page::Window>,
    /// Find duplicates: the search and its results page.
    duplicates: Option<dup_page::Window>,
    /// Convert and resize several images: the choices, then the work.
    batch: Option<batch_page::Window>,
    /// The choices of the last one, kept while the program is open.
    batch_form: Option<batch_page::Form>,
    /// The slideshow under way, at full screen.
    show: Option<slideshow::Show>,
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
    /// New folder: name, the folder it goes in, first frame (the name gets selected).
    new_folder: Option<(String, String, bool)>,
    /// A folder just made: selected once its folder is read again.
    select_after: Option<String>,
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
    /// Asked to be read again while a listing was on its way: that one may be from before the
    /// change, so it is read once more when it arrives.
    relist_again: HashSet<String>,
    /// Item chosen from a right-click menu, run when the frame is drawn (never during the grid).
    act: Option<Act>,
}

impl App {
    /// `arg`: address from the command line. Without it (e.g. started from the applications menu)
    /// the last folder viewed if the option is on, otherwise the home folder.
    #[allow(clippy::too_many_lines)] // constructor: one line per field
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
            video: None,
            download: None,
            restart_error: None,
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
            preparing: None,
            skipped: Vec::new(),
            batch_saved: None,
            fresh: None,
            after_turn: None,
            after_save: None,
            notice: None,
            kiod_ask: false,
            host_ask: false,
            server_manager: None,
            unlock: None,
            saved_servers: servers::load(),
            last_dirs: HashMap::new(),
            status_note: None,
            note_until: None,
            stall_notice: None,
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
            rotate: None,
            duplicates: None,
            batch: None,
            batch_form: None,
            show: None,
            maintained: HashSet::new(),
            delete_confirm: None,
            delete_answer: None,
            rename: None,
            new_folder: None,
            select_after: None,
            restore_queue: Default::default(),
            restore_busy: false,
            restore_conflict: None,
            watcher: None,
            watched: None,
            changed_at: None,
            relisting: HashSet::new(),
            relist_again: HashSet::new(),
            act: None,
        };
        app.watcher = watch::folder_watcher(app.tx.clone(), ctx.clone());
        let wake = ctx.clone();
        mtp::on_change(move || wake.request_repaint());
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

}

impl App {
    /// Every frame, before drawing: messages, retries, keys, title and window size.
    fn before_frame(&mut self, ctx: &egui::Context) {
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
        // Closing with an image turned and not saved: asked first, as when leaving the image.
        if self.transfer.is_none()
            && ctx.input(|i| i.viewport().close_requested())
            && self.hold_for_turn(save_dialogs::After::Quit)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.receive(ctx);
        self.phone_events(ctx);
        self.phone_stall();
        self.notice_gone(ctx);
        self.retry(ctx);
        self.reread_changed(ctx);
        // "Save in another format" and the duplicates cover the window: the keys must not reach the grid under them.
        if self.convert.is_none() && self.rotate.is_none() && self.show.is_none() && !self.dup_page_shown() {
            self.global_keys(ctx);
            if self.viewing.is_some() {
                self.viewer_keys(ctx);
            }
        }
        if let Some(current) = self.viewing {
            self.prefetch_full(current);
        }
        self.update_title(ctx);
        // Size to remember: that of the non-maximised window.
        let (size, maximized) = ctx.input(|i| (i.viewport_rect().size(), i.viewport().maximized));
        self.settings.window_maximized = maximized.unwrap_or(false);
        if !self.settings.window_maximized {
            self.settings.window_size = Some([size.x, size.y]);
        }
    }

    /// The panels of the window: the "Save in another format" page, or the bars with the viewer
    /// or with the tree, the metadata and the grid.
    fn panels(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.show.is_some() {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| self.show_page(ui));
        } else if self.convert.is_some() {
            egui::CentralPanel::default().show(ui, |ui| self.convert_page(ui));
        } else if self.rotate.is_some() {
            egui::CentralPanel::default().show(ui, |ui| self.rotate_page(ui));
        } else if self.dup_page_shown() {
            self.dup_page(ui);
        } else {
            egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
            egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, ctx));
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
                    self.video = None;
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
                        self.tab(ctx, back);
                    }
                }
            }
        }
    }

    /// The windows over the panels (manual, dialogs, confirmations) and the chosen menu item.
    fn windows(&mut self, ctx: &egui::Context) {
        let mut open = self.help;
        egui::Window::new(self.i18n.t("menu-manual"))
            .open(&mut open)
            .default_size([560.0, 480.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    CommonMarkViewer::new().show(ui, &mut self.help_cache, self.i18n.manual);
                });
            });
        self.help = open;
        self.about_window(ctx);
        self.update_window(ctx);
        self.options_window(ctx);
        if let Some(a) = self.act.take() {
            self.run(ctx, a);
        }
        self.duplicates_window(ctx);
        self.batch_window(ctx);
        self.delete_window(ctx);
        self.turn_leave_window(ctx);
        self.save_window(ctx);
        self.batch_turn_window(ctx);
        self.notice_window(ctx);
        self.kiod_window(ctx);
        self.host_window(ctx);
        self.address_window(ctx);
        self.server_manager_window(ctx);
        self.unlock_window(ctx);
        self.paste_window(ctx);
        self.transfer_window(ctx);
        self.rename_window(ctx);
        self.new_folder_window(ctx);
        self.restore_window(ctx);
        self.next_restore();
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // Confirmation open: the keys answer it and reach nothing else (Esc would close the viewer).
        // Open location and Convert and resize have fields to type in: there the keys go to the field.
        if self.asking() && !((self.open_address.is_some() || self.server_manager.is_some() || self.unlock.is_some() || self.batch.is_some()) && ctx.egui_wants_keyboard_input()) {
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
        if ctx.egui_wants_keyboard_input() || self.rename.is_some() || self.new_folder.is_some() {
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
        mtp::Phone::let_go_all();
        video_read::forget_copies();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.before_frame(&ctx);
        self.panels(ui, &ctx);
        self.windows(&ctx);

        #[cfg(debug_assertions)]
        debug_shot::debug_screenshot(&ctx);
    }
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
    // Flatpak points XDG_DATA_HOME into the sandbox: files would go to a trash no file manager sees.
    let flatpak_id = std::env::var("FLATPAK_ID").ok();
    if let (Some(_), Some(home)) = (&flatpak_id, std::env::var_os("HOME")) {
        // SAFETY: no other thread is running yet.
        unsafe { std::env::set_var("XDG_DATA_HOME", std::path::Path::new(&home).join(".local/share")) };
    }
    let settings = Settings::load();
    engine::set_choice(settings.video_engine);
    // The window is matched to its .desktop file by this id (in Flatpak, the id of the app).
    let app_id = flatpak_id.unwrap_or_else(|| "eyecansee".into());
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_app_id(app_id);
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
    fn version_matches_cargo() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"), "VERSION and Cargo.toml must match");
    }
}
