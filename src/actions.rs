//! Menu items and keys that act on the whole app: what each item of the menu bar and of the
//! right-click menus does, F1, F2, F5 and Delete, thumbnails made again.

use eframe::egui::{self, Key};

use crate::address::AddressForm;
use crate::loader::Msg;
use crate::navigation::Go;
use crate::{App, browser, meta, settings, thumbcache, transform, update};

/// Items of the menu bar and of the right-click menus: the menus only record the choice, which
/// runs afterwards (`App::run`) with access to the whole app.
#[derive(Debug)]
pub(crate) enum Act {
    /// A submenu of a right-click menu (Transform): never run itself.
    Submenu(Vec<browser::MenuItem>),
    /// A right-click item shown greyed out, with why on hover: never run.
    Off(String),
    Address,
    Options,
    /// Ctrl+A in the grid.
    SelectAll,
    /// Item (`None` = the selected one or the one in the viewer); `true`: for good (Shift+Del).
    Delete(Option<String>, bool),
    Rename(Option<String>),
    /// A new folder in the folder open.
    /// In that folder, or in the folder open.
    NewFolder(Option<String>),
    /// "Save in another format" for an image (`None` = the selected one or the one in the viewer).
    Convert(Option<String>),
    /// Convert and resize images of the grid (`None` = the marked ones or the selected one).
    Batch(Option<String>),
    /// Slideshow of the marked images, or of the folder from this one (`None` = the selected one
    /// or the one in the viewer).
    Slideshow(Option<String>),
    /// Find duplicates in a folder (`None` = the one open).
    Duplicates(Option<String>),
    /// Makes the thumbnails again (`None` = the selected images or the one in the viewer).
    RedoThumbs(Option<String>),
    /// Shift+F5: every thumbnail of the open folder made again, and the folder read again.
    RedoAllThumbs,
    /// A flip or quarter turn: in the viewer shown until saved; in the grid the item (`None` = the
    /// selected or marked images) saved straight away, after a confirmation.
    Transform(transform::Op, Option<String>),
    /// Rotate by any angle and crop, one image (`None` = the selected one or the one in the viewer).
    RotateCrop(Option<String>),
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

impl App {
    pub(crate) fn global_keys(&mut self, ctx: &egui::Context) {
        let (help, refresh, delete, shift, rename) = ctx.input(|i| {
            (i.key_pressed(Key::F1), i.key_pressed(Key::F5), i.key_pressed(Key::Delete), i.modifiers.shift, i.key_pressed(Key::F2))
        });
        // Ctrl+Shift+N, as in the file managers.
        let new_folder = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::N));
        if new_folder && !ctx.egui_wants_keyboard_input() {
            self.ask_new_folder(None);
        }
        if !ctx.egui_wants_keyboard_input() && self.rename.is_none() && self.new_folder.is_none() {
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

    /// Thumbnails gone from the disk cache, then from memory (`Msg::Forgotten`): the grid asks for
    /// them again and they are made from the file (a wrong one stays otherwise, since size and
    /// date do not change).
    pub(crate) fn redo_thumbs(&mut self, ctx: &egui::Context, paths: Vec<String>) {
        let (tx, ctx, back) = (self.tx.clone(), ctx.clone(), paths.clone());
        thumbcache::forget(&self.base, paths, move || {
            let _ = tx.send(Msg::Forgotten(back));
            ctx.request_repaint();
        });
    }

    pub(crate) fn redo_all_thumbs(&mut self, ctx: &egui::Context) {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        self.redo_thumbs(ctx, l.files.iter().map(|f| f.path.clone()).collect());
        self.refresh();
    }

    /// Runs an item chosen from the menu bar or from a right-click menu.
    pub(crate) fn run(&mut self, ctx: &egui::Context, act: Act) {
        log!("menu {act:?}");
        match act {
            Act::Address => self.open_address = Some(AddressForm { first: true, ..Default::default() }),
            Act::Options => self.options = true,
            Act::SelectAll => self.select_all(),
            Act::Delete(path, permanent) => self.ask_delete(path, permanent),
            Act::Rename(path) => self.ask_rename(path),
            Act::NewFolder(dir) => self.ask_new_folder(dir),
            Act::Convert(path) => self.open_convert(path),
            Act::Duplicates(dir) => self.open_duplicates(dir),
            Act::Batch(path) => self.open_batch(path),
            Act::Slideshow(path) => self.start_show(ctx, path),
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
            Act::RotateCrop(path) => self.open_rotate(path),
            Act::Save => self.ask_save(),
            Act::Submenu(_) | Act::Off(_) => {}
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
            Act::Open => self.open_selected(),
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
}
