//! A folder chosen inside EyeCanSee, among the same devices as the left panel. The system window
//! did not reach a phone opened directly (on KDE it cannot see it at all) and on GNOME it went
//! through the slow gvfs path.

use std::sync::{Arc, Mutex};

use eframe::egui;
use tokio::runtime::Handle;

use crate::App;
use crate::i18n::I18n;
use crate::icons::{self, Icon};
use crate::source::{self, Store};
use crate::confirm::FollowArea;

/// Height of the two columns, in points.
const HEIGHT: f32 = 340.0;

type Dirs = Arc<Mutex<Option<Result<Vec<String>, source::Error>>>>;

#[cfg(test)]
thread_local! {
    /// Names of subfolders copied to draw the chooser on this thread (audit 1.14: all of them at
    /// every frame).
    static COPIED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The folder being looked at and its subfolders, read in the background.
#[derive(Clone)]
pub struct Picker {
    base: String,
    /// `None` until a place opens.
    store: Option<Store>,
    dir: String,
    dirs: Dirs,
    /// The address it opened at, until its first listing answers: if that fails, the home
    /// folder instead (a phone unplugged since it was chosen opens, then fails to list).
    first: Option<String>,
    /// The folder a server starts in, once it answers (`go_home`).
    found: Arc<Mutex<Option<String>>>,
    /// New folder: the name being typed, and whether its window opens this frame.
    naming: Option<(String, bool)>,
    /// The folder made (gone into once it answers), or why not.
    made: Arc<Mutex<Option<Result<String, source::Error>>>>,
    made_error: Option<source::Error>,
}

/// Where the chooser can jump to: home, the computer, each device, each saved server.
pub struct Place {
    icon: Icon,
    label: String,
    address: String,
}

impl Picker {
    /// At `address`; if it cannot be read (a phone unplugged since it was chosen, a folder gone),
    /// at the home folder (`settle`). An error is shown in place of the folders only for a place
    /// chosen in the window, or if the home folder fails too.
    pub fn open(address: &str, rt: &Handle, ctx: &egui::Context) -> Picker {
        let mut p = Picker { base: String::new(), store: None, dir: String::new(), dirs: Arc::default(), first: None, found: Arc::default(), naming: None, made: Arc::default(), made_error: None };
        p.place(address, rt, ctx);
        p.first = Some(p.address());
        p
    }

    /// Once the first listing answers: if it failed and the window is still there, `home` (the
    /// home folder; a folder of its own in the test, whose runner never finished listing its home).
    fn settle(&mut self, home: Option<std::path::PathBuf>, rt: &Handle, ctx: &egui::Context) {
        if let Some(dir) = self.found.lock().unwrap().take() {
            self.dir = dir;
        }
        let made = self.made.lock().unwrap().take();
        match made {
            Some(Ok(dir)) => self.go(dir, rt, ctx),
            Some(Err(e)) => self.made_error = Some(e),
            None => {}
        }
        let failed = match &*self.dirs.lock().unwrap() {
            None => return,
            Some(r) => r.is_err(),
        };
        let first = self.first.take();
        if failed
            && first == Some(self.address())
            && let Some(home) = home
        {
            log!("pick: {first:?} cannot be read, the home folder instead");
            self.place(&home.to_string_lossy(), rt, ctx);
        }
    }

    fn place(&mut self, address: &str, rt: &Handle, ctx: &egui::Context) {
        log!("pick place {:?}", crate::address::without_password(address));
        match source::open(address) {
            Ok(s) => {
                (self.base, self.store) = (s.base, Some(s.op));
                if s.home { self.go_home(rt, ctx) } else { self.go(s.dir, rt, ctx) }
            }
            Err(e) => self.dirs = Arc::new(Mutex::new(Some(Err(e)))),
        }
    }

    /// Into `dir`: its subfolders are read in the background. A new slot each time, so an
    /// answer arriving late for the folder left behind goes nowhere.
    fn go(&mut self, dir: String, rt: &Handle, ctx: &egui::Context) {
        let dirs: Dirs = Arc::default();
        self.dirs = dirs.clone();
        (self.found, self.naming, self.made_error) = (Arc::default(), None, None);
        self.dir = dir.clone();
        let Some(store) = self.store.clone() else { return };
        let ctx = ctx.clone();
        rt.spawn(async move {
            let r = store.list(&dir).await.map(|l| l.dirs);
            log!("pick {dir:?}: {:?}", r.as_ref().map(Vec::len));
            *dirs.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    /// What is wrong with `name` for a new folder here, as for New folder in the grid (the files
    /// of the folder are not listed here: the server says so if one has that name).
    fn name_problem(&self, name: &str, i18n: &I18n) -> Option<String> {
        let taken = self.dirs.lock().unwrap().as_ref().and_then(|r| r.as_ref().ok()).is_some_and(|d| d.iter().any(|p| source::name(p) == name));
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            Some(i18n.t("rename-bad"))
        } else if taken {
            Some(i18n.t("rename-taken"))
        } else {
            None
        }
    }

    /// The folder `name` made in the folder looked at, in the background (`settle` goes into it).
    fn make(&mut self, name: &str, rt: &Handle, ctx: &egui::Context) {
        let Some(store) = self.store.clone() else { return };
        let (made, ctx, path) = (self.made.clone(), ctx.clone(), format!("{}{name}/", self.dir));
        (self.naming, self.made_error) = (None, None);
        rt.spawn(async move {
            let r = store.create_dir(&path).await.map(|()| path.clone());
            log!("pick: new folder {path:?}: {:?}", r.as_ref().err());
            *made.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    /// A server opened without a folder: into the folder it starts in, as the grid does
    /// (`loader.home`). Its root may not be readable: a Hetzner Storage Box refuses "/" over SFTP.
    fn go_home(&mut self, rt: &Handle, ctx: &egui::Context) {
        let (dirs, found): (Dirs, Arc<Mutex<Option<String>>>) = (Arc::default(), Arc::default());
        (self.dirs, self.found, self.dir) = (dirs.clone(), found.clone(), String::new());
        (self.naming, self.made_error) = (None, None);
        let Some(store) = self.store.clone() else { return };
        let ctx = ctx.clone();
        rt.spawn(async move {
            let home = match &store {
                Store::Sftp(server) => server.home().await,
                Store::Ftp(server) => server.home().await,
                _ => Ok(String::new()),
            };
            let r = match home {
                Ok(home) => {
                    let r = store.list(&home).await.map(|l| l.dirs);
                    log!("pick home {home:?}: {:?}", r.as_ref().map(Vec::len));
                    *found.lock().unwrap() = Some(home);
                    r
                }
                Err(e) => Err(e),
            };
            *dirs.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    /// Its folders are listed (or failed): what is in it can be gone through.
    #[cfg(test)]
    pub(crate) fn dirs_listed(&self) -> bool {
        self.dirs.lock().unwrap().is_some()
    }

    /// The name of a new folder being typed, if any.
    pub(crate) fn naming(&self) -> Option<&str> {
        self.naming.as_ref().map(|(name, _)| name.as_str())
    }

    /// New folder: its name asked, a free one proposed as in the grid.
    pub(crate) fn ask_name(&mut self, i18n: &I18n) {
        let taken = self.dirs.lock().unwrap().as_ref().and_then(|r| r.as_ref().ok()).map(|d| d.iter().map(|p| source::name(p).to_string()).collect()).unwrap_or_default();
        self.naming = Some((source::free_name(&taken, &i18n.t("new-folder-name")), true));
    }

    /// As the address bar takes it.
    pub fn address(&self) -> String {
        crate::navigation::shown(&self.base, &self.dir)
    }
}

impl App {
    pub(crate) fn pick_places(&self) -> Vec<Place> {
        let mut places = Vec::new();
        if let Some(home) = std::env::home_dir() {
            let home = home.to_string_lossy().into_owned();
            places.push(Place { icon: Icon::Home, label: self.i18n.t("pick-home"), address: home });
        }
        // No "This computer" (/) on Linux: next to the home folder it looked like the same place,
        // and it is two clicks up from there.
        // Unmounted partitions have no folder to choose yet.
        for d in self.devices.iter().filter(|d| d.block.is_none()) {
            let icon = if d.phone { Icon::Phone } else if d.internal { Icon::InternalDisk } else { Icon::Disk };
            places.push(Place { icon, label: d.name.clone(), address: d.path.clone() });
        }
        // The saved servers (File > Server manager), as in the left panel.
        for s in &self.saved_servers {
            if let Some(address) = s.address() {
                places.push(Place { icon: Icon::Server, label: s.name.clone(), address });
            }
        }
        places
    }
}

/// The chooser; `Some(Some(address))` once a folder is chosen, `Some(None)` cancelled.
/// `keys`: Enter and Esc from the window; `key`, those left for the new folder's name over it
/// (`App::key_answer`).
pub fn pick_ui(ui: &mut egui::Ui, p: &mut Picker, places: &[Place], i18n: &I18n, rt: &Handle, keys: Option<bool>, key: &mut Option<bool>) -> Option<Option<String>> {
    let ctx = ui.ctx().clone();
    p.settle(std::env::home_dir(), rt, &ctx);
    crate::theme::title(ui, i18n.t("pick-title"));
    ui.add_space(8.0);
    // The place the folder looked at is in: the longest address in front of it.
    let current = p.address();
    let at = places.iter().enumerate().filter(|(_, pl)| current.starts_with(&pl.address)).max_by_key(|(_, pl)| pl.address.len()).map(|(i, _)| i);
    ui.horizontal_top(|ui| {
        boxed(ui, 160.0, |ui| {
            egui::ScrollArea::vertical().id_salt("pick-places").auto_shrink([false, false]).show_follow(ui, |ui| {
                for (i, place) in places.iter().enumerate() {
                    let label = (icons::image(&ctx, place.icon, 16.0), place.label.as_str());
                    if ui.selectable_label(at == Some(i), label).on_hover_text(&place.address).clicked() {
                        p.place(&place.address, rt, &ctx);
                    }
                }
            });
        });
        // What is left of the row, less the margins of the box.
        boxed(ui, ui.available_width() - 18.0, |ui| folders_ui(ui, p, i18n, rt));
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    // Not before the folder answers: a server's starting folder is known only then.
    let answered = p.dirs.lock().unwrap().is_some();
    // While a new folder's name is typed, Enter and Esc are its own.
    let keys = keys.filter(|_| p.naming.is_none());
    let mut answer = keys.filter(|&yes| answered || !yes).map(|yes| yes.then(|| p.address()));
    ui.horizontal(|ui| {
        let ok = crate::theme::main_button(ui, i18n.t("pick-use"));
        if ui.add_enabled(answered, ok).clicked() {
            answer = Some(Some(p.address()));
        }
        if ui.button(i18n.t("delete-no")).clicked() {
            answer = Some(None);
        }
    });
    naming_window(&ctx, p, i18n, rt, key);
    answer
}

/// A framed column. Fixed sizes: in a modal window the columns would otherwise grow without end.
fn boxed(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    // Top to bottom: the row around would lay the items side by side.
    egui::Frame::group(ui.style()).inner_margin(8.0).show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            ui.set_width(width);
            ui.set_height(HEIGHT);
            add(ui);
        });
    });
}

/// The folder looked at, the way up, and its subfolders: a click goes in.
fn folders_ui(ui: &mut egui::Ui, p: &mut Picker, i18n: &I18n, rt: &Handle) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        let up = egui::Button::image(icons::line(ui, icons::Line::Up, 16.0));
        if ui.add_enabled(!p.dir.is_empty(), up).on_hover_text(i18n.t("pick-up")).clicked() {
            p.go(source::parent(&p.dir), rt, &ctx);
        }
        // Not among the storages of a phone opened directly, as New folder in the grid.
        let listed = matches!(&*p.dirs.lock().unwrap(), Some(Ok(_)));
        let can = listed && p.naming.is_none() && !(p.base.starts_with("mtp://") && p.dir.is_empty());
        let new = egui::Button::image(icons::line(ui, icons::Line::NewFolder, 16.0));
        if ui.add_enabled(can, new).on_hover_text(i18n.t("menu-new-folder")).clicked() {
            p.ask_name(i18n);
        }
        ui.add(egui::Label::new(egui::RichText::new(p.address()).strong()).truncate());
    });
    if let Some(e) = &p.made_error {
        crate::error_bar::inline(ui, &i18n.error(e.0, &e.1));
    }
    ui.add_space(2.0);
    ui.separator();
    let dirs = p.dirs.lock().unwrap().clone();
    #[cfg(test)]
    COPIED.with(|n| n.set(n.get() + dirs.as_ref().and_then(|d| d.as_ref().ok()).map_or(0, Vec::len)));
    egui::ScrollArea::vertical().id_salt("pick-dirs").auto_shrink([false, false]).show_follow(ui, |ui| match dirs {
        None => {
            ui.spinner();
        }
        Some(Err((id, detail))) => {
            crate::error_bar::inline(ui, &i18n.error(id, &detail));
        }
        Some(Ok(dirs)) if dirs.is_empty() => {
            ui.weak(i18n.t("pick-empty"));
        }
        Some(Ok(dirs)) => {
            for dir in dirs {
                if ui.selectable_label(false, (icons::image(&ctx, Icon::Folder, 16.0), source::name(&dir))).clicked() {
                    p.go(dir, rt, &ctx);
                }
            }
        }
    });
}

/// The name of a new folder, in a window over the chooser as New folder in the grid (#311: typed
/// in a line of the chooser, it took no keys from Save as…). Create or Enter makes it, Esc or
/// Cancel leaves it; what is wrong with the name under it.
fn naming_window(ctx: &egui::Context, p: &mut Picker, i18n: &I18n, rt: &Handle, key: &mut Option<bool>) {
    let Some((mut name, first)) = p.naming.clone() else { return };
    let problem = p.name_problem(name.trim(), i18n);
    let texts = (i18n.t("new-folder-title"), i18n.t("new-folder-ok"), i18n.t("delete-no"));
    let select = first.then(|| name.chars().count());
    match crate::file_dialogs::name_window(ctx, "pick-new-folder", texts, &mut name, select, problem, key) {
        Some(true) => p.make(name.trim(), rt, ctx),
        Some(false) => p.naming = None,
        None => p.naming = Some((name, false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subfolders_and_up() {
        let root = std::env::temp_dir().join(format!("eyecansee-pick-{}", std::process::id()));
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("x.jpg"), "x").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let wait = |p: &Picker| loop {
            if let Some(r) = p.dirs.lock().unwrap().clone() {
                break r.unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let mut p = Picker::open(&root.to_string_lossy(), rt.handle(), &ctx);
        let dirs = wait(&p);
        assert_eq!(dirs.iter().map(|d| source::name(d)).collect::<Vec<_>>(), ["a", "b"], "folders only, in order");
        p.go(dirs[0].clone(), rt.handle(), &ctx);
        assert!(wait(&p).is_empty());
        // Both canonical: on Windows one is "C:\…", the other "\\?\C:\…".
        assert_eq!(std::path::Path::new(&p.address()).canonicalize().unwrap(), root.canonicalize().unwrap().join("a"));
        p.go(source::parent(&p.dir), rt.handle(), &ctx);
        assert_eq!(wait(&p).len(), 2);
        // New folder: a name taken or not valid is refused; a new one is made and gone into.
        let i18n = I18n::new("en");
        assert!(p.name_problem("a", &i18n).is_some() && p.name_problem("..", &i18n).is_some() && p.name_problem("x/y", &i18n).is_some());
        assert!(p.name_problem("c", &i18n).is_none());
        p.make("c", rt.handle(), &ctx);
        while p.made.lock().unwrap().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        p.settle(None, rt.handle(), &ctx);
        assert!(wait(&p).is_empty() && root.join("c").is_dir());
        assert_eq!(source::name(&p.dir), "c");
        p.go(source::parent(&p.dir), rt.handle(), &ctx);
        assert_eq!(wait(&p).len(), 3);
        // A place that does not exist: the error in place of the folders, no panic.
        p.place("/does/not/exist", rt.handle(), &ctx);
        assert!(p.dirs.lock().unwrap().as_ref().unwrap().is_err());
        // Opened there (a phone unplugged since): the home folder instead.
        let mut p = Picker::open("/does/not/exist", rt.handle(), &ctx);
        p.settle(Some(root.clone()), rt.handle(), &ctx);
        assert_eq!(wait(&p).len(), 3, "a, b and the new c");
        assert_eq!(std::path::Path::new(&p.address()).canonicalize().unwrap(), root.canonicalize().unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Audit 1.14: the chooser draws the subfolders without copying their list at every frame.
    #[test]
    fn subfolders_drawn_without_a_copy() {
        let root = std::env::temp_dir().join(format!("eyecansee-pick-copy-{}", std::process::id()));
        for i in 0..50 {
            std::fs::create_dir_all(root.join(format!("d{i:02}"))).unwrap();
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let mut p = Picker::open(&root.to_string_lossy(), rt.handle(), &ctx);
        while p.dirs.lock().unwrap().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let i18n = I18n::new("en");
        COPIED.with(|n| n.set(0));
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| folders_ui(ui, &mut p, &i18n, rt.handle()));
            out.textures_delta.clear();
        }
        let copied = COPIED.with(|n| n.get());
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(copied, 0, "{copied} names of subfolders copied in two frames");
    }
}
