//! The App's constructor: the first value of every field, the watchers started and the first
//! folder opened (the address given, the last one viewed or the home folder).

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

use eframe::egui;

use crate::address::AddressForm;
use crate::fonts::fonts;
use crate::i18n::{self, I18n};
use crate::settings::Settings;
use crate::{App, Focus, OptTab, devices, help, help_icons, mtp, servers, single, theme, watch};

impl App {
    /// `arg`: address from the command line. Without it (e.g. started from the applications menu)
    /// the last folder viewed if the option is on, otherwise the home folder.
    #[allow(clippy::too_many_lines)] // constructor: one line per field
    pub(crate) fn new(ctx: &egui::Context, settings: Settings, arg: Option<String>) -> Self {
        ctx.set_fonts(fonts());
        help_icons::install(ctx);
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
            keys_open: false,
            start: None,
            thumbs: HashMap::new(),
            thumb_keep: 0..0,
            generated: HashSet::new(),
            thumb_queue_key: None,
            full: HashMap::new(),
            model: Default::default(),
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
            bookmarks: Default::default(),
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
            kept: Vec::new(),
            title: String::new(),
            help: help::Guide::default(),
            about: false,
            clip: None,
            paste_to: None,
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
            editor: None,
            animated: Default::default(),
            duplicates: None,
            batch: None,
            batch_form: None,
            show: None,
            maintained: HashSet::new(),
            delete_confirm: None,
            delete_check: None,
            delete_answer: None,
            rename: None,
            new_folder: None,
            new_image: None,
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
            (Some(arg), _) => {
                app.open_outside(ctx, &arg);
                // An address that does not open (a folder the user may not read, say): its error,
                // with home behind it; without a source the tree and the grid spun for ever (#105).
                if app.loader.is_none() {
                    let error = app.error.take();
                    app.open_source(ctx, &home);
                    app.error = error.or(app.error.take());
                }
            }
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
                let phone_gone = devices::phone_missing(&last, &app.devices);
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
