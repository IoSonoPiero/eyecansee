//! Save as…: the folder the new file goes into. The original's at first; another one, also on
//! another device, chosen with the folder chooser of Convert… (`folder_pick`, issue #127).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use eframe::egui;

use super::Window;
use crate::App;
use crate::folder_pick::{Picker, pick_ui};
use crate::i18n::I18n;
use crate::source::{self, Store};

/// A folder other than the original's.
#[derive(Clone)]
pub(crate) struct Folder {
    pub store: Store,
    pub base: String,
    pub dir: String,
    /// The names already there, read in the background once chosen: the page says before saving
    /// which name the file will get (`write_free` checks again when writing).
    names: Arc<Mutex<Option<HashSet<String>>>>,
}

impl Folder {
    fn new(s: source::Source, rt: &tokio::runtime::Handle, ctx: &egui::Context) -> Folder {
        let names: Arc<Mutex<Option<HashSet<String>>>> = Arc::default();
        let (store, dir, slot, ctx) = (s.op.clone(), s.dir.clone(), names.clone(), ctx.clone());
        rt.spawn(async move {
            let r = store.names(&dir).await;
            log!("save as: {} names in {dir:?}", r.as_ref().map_or(0, HashSet::len));
            *slot.lock().unwrap() = Some(r.unwrap_or_default());
            ctx.request_repaint();
        });
        Folder { store: s.op, base: s.base, dir: s.dir, names }
    }

    pub(crate) fn taken(&self) -> HashSet<String> {
        self.names.lock().unwrap().clone().unwrap_or_default()
    }
}

impl Window {
    /// The place the image is in: the editor's (`place`), or the source open (`open`).
    pub(crate) fn original_base<'a>(&'a self, open: &'a str) -> &'a str {
        self.place.as_ref().map_or(open, |p| p.base.as_str())
    }

    /// The place the new file goes to: the folder chosen, or the image's.
    pub(crate) fn dest_base<'a>(&'a self, open: &'a str) -> &'a str {
        self.folder.as_ref().map_or_else(|| self.original_base(open), |f| f.base.as_str())
    }

    /// The folder the new file goes into: the one chosen, or the original's.
    pub(crate) fn dest_dir(&self) -> String {
        self.folder.as_ref().map_or_else(|| source::parent(&self.path), |f| f.dir.clone())
    }

    /// The store the new file is written to by the page itself; `None`: the source open
    /// (`Loader::save`).
    pub(crate) fn dest_store(&self) -> Option<&Store> {
        self.folder.as_ref().map(|f| &f.store).or(self.place.as_ref().map(|p| &p.store))
    }

    /// The folder chooser is open over the page: Enter and Esc answer it.
    pub fn asking(&self) -> bool {
        self.picker.is_some()
    }
}

impl App {
    /// The folder chooser, after Browse…; once a folder is chosen, the new file goes there.
    pub(super) fn convert_pick(&mut self, ctx: &egui::Context, w: &mut Window) {
        let rt = self.rt.handle().clone();
        if std::mem::take(&mut w.browse) {
            let shown = crate::navigation::shown(w.dest_base(&self.base), &w.dest_dir());
            w.picker = Some(Picker::open(&shown, &rt, ctx));
        }
        let Some(p) = &mut w.picker else { return };
        let keys = self.delete_answer.take();
        let places = self.pick_places();
        let mut picked = None;
        egui::Modal::new(egui::Id::unique("save-as-pick")).show(ctx, |ui| {
            ui.set_width(680.0);
            picked = pick_ui(ui, p, &places, &self.i18n, &rt, keys);
        });
        let Some(choice) = picked else { return };
        w.picker = None;
        let Some(address) = choice else { return };
        match other_folder(&address, w.original_base(&self.base), &source::parent(&w.path)) {
            Ok(s) => {
                log!("save as: folder {:?}", crate::address::without_password(&address));
                w.folder = s.map(|s| Folder::new(s, &rt, ctx));
                w.error = None;
            }
            Err((id, detail)) => w.error = Some(self.i18n.error(id, &detail)),
        }
    }
}

/// The folder at `address`, as the chooser gives it; `None` when it is the original's (`base`,
/// `dir`), which needs nothing more.
fn other_folder(address: &str, base: &str, dir: &str) -> Result<Option<source::Source>, source::Error> {
    let s = source::open(address)?;
    if s.start.is_some() {
        return Err(("batch-not-folder", address.to_string()));
    }
    Ok((s.base != base || s.dir != dir).then_some(s))
}

/// The folder the file goes into, and Browse… to choose another (the chooser opens after drawing).
pub(super) fn folder_row(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n, shown: &str) {
    ui.horizontal(|ui| {
        ui.label(i18n.t("convert-folder"));
        // What the row leaves after the button: a long address is cut, whole when pointed at.
        let room = (ui.available_width() - 110.0).max(80.0);
        ui.scope(|ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(egui::RichText::new(shown).strong()).truncate()).on_hover_text(shown);
        });
        if ui.add_enabled(!w.saving, egui::Button::new(i18n.t("batch-browse"))).clicked() {
            w.browse = true;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #127: the folder chosen is used unless it is the original's; a file is not a folder.
    #[test]
    fn another_folder_or_the_originals() {
        let root = std::env::temp_dir().join(format!("eyecansee-save-as-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("there")).unwrap();
        std::fs::write(root.join("a.jpg"), b"x").unwrap();
        let here = source::open(&root.to_string_lossy()).unwrap();
        let address = |p: &str| root.join(p).to_string_lossy().into_owned();
        assert!(other_folder(&root.to_string_lossy(), &here.base, &here.dir).unwrap().is_none());
        let there = other_folder(&address("there"), &here.base, &here.dir).unwrap().unwrap();
        assert_eq!(there.dir, format!("{}there/", here.dir));
        assert_eq!(other_folder(&address("a.jpg"), &here.base, &here.dir).err().map(|e| e.0), Some("batch-not-folder"));
        assert!(other_folder(&address("gone"), &here.base, &here.dir).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Issue #127: the file goes into the folder chosen, with its names; otherwise beside the
    /// original, in the editor's place when opened from the editor.
    #[test]
    fn where_the_file_goes() {
        let root = std::env::temp_dir().join(format!("eyecansee-save-as-dest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("b.png"), b"x").unwrap();
        let mut w = Window::new("p/a.jpg".into());
        assert_eq!((w.dest_dir(), w.dest_base("/"), w.dest_store().is_none()), ("p/".into(), "/", true));
        let server = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&root.to_string_lossy())).unwrap());
        w.place = Some(crate::edit_page::Place { store: server, base: "sftp://u@server/".into() });
        assert_eq!((w.dest_dir(), w.dest_base("/"), w.dest_store().is_some()), ("p/".into(), "sftp://u@server/", true));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let f = Folder::new(source::open(&root.to_string_lossy()).unwrap(), rt.handle(), &ctx);
        let names = loop {
            if let Some(n) = f.names.lock().unwrap().clone() {
                break n;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(names.contains("b.png"));
        let dir = f.dir.clone();
        w.folder = Some(f);
        assert_eq!((w.dest_dir(), w.dest_base("/"), w.original_base("/")), (dir, "/", "sftp://u@server/"));
        assert_eq!(crate::source::free_name(&w.folder.as_ref().unwrap().taken(), "b.png"), "b (2).png");
        std::fs::remove_dir_all(root).unwrap();
    }
}
