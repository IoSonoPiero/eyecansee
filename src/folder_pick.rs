//! A folder chosen inside EyeCanSee, among the same devices as the left panel. The system window
//! did not reach a phone opened directly (on KDE it cannot see it at all) and on GNOME it went
//! through the slow gvfs path.

use std::sync::{Arc, Mutex};

use eframe::egui;
use tokio::runtime::Handle;

use crate::App;
use crate::browser::error_text;
use crate::i18n::I18n;
use crate::icons::{self, Icon};
use crate::source::{self, Store};

/// Height of the two columns, in points.
const HEIGHT: f32 = 340.0;

type Dirs = Arc<Mutex<Option<Result<Vec<String>, source::Error>>>>;

/// The folder being looked at and its subfolders, read in the background.
pub struct Picker {
    base: String,
    /// `None` until a place opens.
    store: Option<Store>,
    dir: String,
    dirs: Dirs,
}

/// Where the chooser can jump to: home, the computer, each device.
pub struct Place {
    icon: Icon,
    label: String,
    address: String,
}

impl Picker {
    /// At `address`; if it cannot be opened, the error is shown in place of the folders.
    pub fn open(address: &str, rt: &Handle, ctx: &egui::Context) -> Picker {
        let mut p = Picker { base: String::new(), store: None, dir: String::new(), dirs: Arc::default() };
        p.place(address, rt, ctx);
        p
    }

    fn place(&mut self, address: &str, rt: &Handle, ctx: &egui::Context) {
        match source::open(address) {
            Ok(s) => {
                (self.base, self.store) = (s.base, Some(s.op));
                self.go(s.dir, rt, ctx);
            }
            Err(e) => self.dirs = Arc::new(Mutex::new(Some(Err(e)))),
        }
    }

    /// Into `dir`: its subfolders are read in the background. A new slot each time, so an
    /// answer arriving late for the folder left behind goes nowhere.
    fn go(&mut self, dir: String, rt: &Handle, ctx: &egui::Context) {
        let dirs: Dirs = Arc::default();
        self.dirs = dirs.clone();
        self.dir = dir.clone();
        let Some(store) = self.store.clone() else { return };
        let ctx = ctx.clone();
        rt.spawn(async move {
            let r = store.list(&dir).await.map(|l| l.dirs);
            *dirs.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
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
        places
    }
}

/// The chooser; `Some(Some(address))` once a folder is chosen, `Some(None)` cancelled.
/// `keys`: Enter and Esc from the window.
pub fn pick_ui(ui: &mut egui::Ui, p: &mut Picker, places: &[Place], i18n: &I18n, rt: &Handle, keys: Option<bool>) -> Option<Option<String>> {
    let ctx = ui.ctx().clone();
    ui.heading(i18n.t("pick-title"));
    ui.add_space(8.0);
    // The place the folder looked at is in: the longest address in front of it.
    let current = p.address();
    let at = places.iter().enumerate().filter(|(_, pl)| current.starts_with(&pl.address)).max_by_key(|(_, pl)| pl.address.len()).map(|(i, _)| i);
    ui.horizontal_top(|ui| {
        boxed(ui, 160.0, |ui| {
            egui::ScrollArea::vertical().id_salt("pick-places").auto_shrink([false, false]).show(ui, |ui| {
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
    let mut answer = keys.map(|yes| yes.then(|| p.address()));
    ui.horizontal(|ui| {
        let ok = crate::theme::main_button(ui, i18n.t("pick-use"));
        if ui.add(ok).clicked() {
            answer = Some(Some(p.address()));
        }
        if ui.button(i18n.t("delete-no")).clicked() {
            answer = Some(None);
        }
    });
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
        let up = egui::Button::image(icons::image(&ctx, Icon::UpButton, 16.0));
        if ui.add_enabled(!p.dir.is_empty(), up).on_hover_text(i18n.t("pick-up")).clicked() {
            p.go(source::parent(&p.dir), rt, &ctx);
        }
        ui.add(egui::Label::new(egui::RichText::new(p.address()).strong()).truncate());
    });
    ui.add_space(2.0);
    ui.separator();
    let dirs = p.dirs.lock().unwrap().clone();
    egui::ScrollArea::vertical().id_salt("pick-dirs").auto_shrink([false, false]).show(ui, |ui| match dirs {
        None => {
            ui.spinner();
        }
        Some(Err((id, detail))) => {
            ui.label(error_text(ui, i18n.t_args(id, &[("detail", &detail)])));
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
        assert_eq!(std::path::Path::new(&p.address()), root.canonicalize().unwrap().join("a"));
        p.go(source::parent(&p.dir), rt.handle(), &ctx);
        assert_eq!(wait(&p).len(), 2);
        // A place that does not exist: the error in place of the folders, no panic.
        p.place("/does/not/exist", rt.handle(), &ctx);
        assert!(p.dirs.lock().unwrap().as_ref().unwrap().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
