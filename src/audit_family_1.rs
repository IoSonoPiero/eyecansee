//! Audit of 2026-11-17, family 1 (files of the user that can be lost or overwritten): tests that
//! reproduce the points that need the whole app (windows, sources, the viewer), run without a
//! screen. Ignored until each point is fixed; run them one at a time with
//! `notes/lavori/audit-famiglia-1/script/run.sh`, which gives every run its own settings, cache,
//! data folder (trash) and `EYECANSEE_MULTI=1` (never the instance the user has open). Once a
//! point is fixed its test runs with the others: `own_folder` runs it again in a child process
//! with such a folder.

pub(crate) mod fake_ftp;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::App;
use crate::settings::Settings;

/// The variables that `run.sh` and `own_folder` point into the folder of a run: settings, cache,
/// data (the system trash), temporary files.
const RUN_VARS: &[(&str, &str)] = &[("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data"), ("TMPDIR", "tmp"), ("APPDATA", "config"), ("LOCALAPPDATA", "cache"), ("TEMP", "tmp"), ("TMP", "tmp")];

/// The folder of this run (`AUDIT_ENV`, made by `run.sh` or `own_folder`): the test stops at once
/// if settings, cache or data could be the user's, or if it could talk to the copy of EyeCanSee open.
pub(crate) fn guard() -> PathBuf {
    let env = |v: &str| std::env::var(v).unwrap_or_default();
    let run = env("AUDIT_ENV");
    assert!(!run.is_empty() && run.contains("famiglia1-tools"), "run through run.sh or own_folder: AUDIT_ENV={run:?}");
    let vars: &[&str] = if cfg!(windows) { &["APPDATA", "LOCALAPPDATA", "TEMP"] } else { &["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "TMPDIR"] };
    for v in vars {
        assert!(env(v).starts_with(&run), "{v} not in the folder of this run: {:?}", env(v));
    }
    assert_eq!(env("EYECANSEE_MULTI"), "1", "the single instance would reach the user's copy");
    // On Windows `canonicalize` gives "\\?\C:\…", which is no address a user types or Windows hands
    // to a program: started on it the app could not list it and opened the home folder (CI red from
    // 113f93b, f5_reads_again_only_the_folder_open). The same path written as everywhere else.
    PathBuf::from(PathBuf::from(run).canonicalize().unwrap().to_string_lossy().trim_start_matches(r"\\?\"))
}

/// For a test fixed and no longer ignored, run by `cargo test` with the others: unless it already
/// runs in a folder of its own (`run.sh`, or this child), the test `name` (its path, as `cargo test`
/// lists it) runs again in a child process of this test binary, with settings, cache, data and
/// temporary files in a new folder deleted at the end, and `EYECANSEE_MULTI=1`. `true` in the
/// parent once the child passed: the test returns. A child that fails fails the test.
pub(crate) fn own_folder(name: &str) -> bool {
    if std::env::var_os("AUDIT_ENV").is_some() {
        return false;
    }
    let run = std::env::temp_dir().join(format!("eyecansee-famiglia1-tools-{}-{}", std::process::id(), name.replace("::", "-")));
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", name, "--include-ignored", "--nocapture", "--test-threads=1"]).env("AUDIT_ENV", &run).env("EYECANSEE_MULTI", "1");
    for (var, sub) in RUN_VARS {
        std::fs::create_dir_all(run.join(sub)).unwrap();
        child.env(var, run.join(sub));
    }
    let out = child.output().unwrap();
    let _ = std::fs::remove_dir_all(&run);
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    // `--exact` with a wrong name runs nothing and passes.
    assert!(out.status.success() && text.contains("1 passed"), "{name} in its own folder:\n{text}");
    true
}

/// A folder of this run with small images in it, `names` (PNG data, whatever the name says).
pub(crate) fn folder(name: &str, names: &[&str]) -> PathBuf {
    let dir = guard().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let mut png = Vec::new();
    image::DynamicImage::new_rgb8(8, 6).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    for n in names {
        std::fs::write(dir.join(n), &png).unwrap();
    }
    dir
}

/// The app without a screen, started on `dir` as from the command line.
pub(crate) struct Harness {
    pub ctx: egui::Context,
    pub app: App,
    frame: eframe::Frame,
    /// Full screen was asked of the window (F11), in some frame since the start.
    pub full_asked: bool,
    /// The last full screen asked of the window: in (`true`) or out (`false`) (#358).
    pub full_wanted: Option<bool>,
    /// The window is at full screen, as the window tells the app in every frame (#358).
    pub full: bool,
    /// What the app asked of the window in the last frame.
    pub commands: Vec<egui::ViewportCommand>,
    /// The window's ✕ is pressed in the next frame (`close`).
    closing: bool,
    /// The frames with one of these keys run in two passes, as egui runs a frame when a pass asks
    /// to be discarded (`request_discard`: a new grid, a bar measured); the second pass gets no
    /// events (#360).
    pub two_passes: Vec<egui::Key>,
}

impl Harness {
    pub fn new(dir: &Path) -> Self {
        Self::open(&format!("{}/", dir.display()))
    }

    /// Started on `address`, a folder.
    pub fn open(address: &str) -> Self {
        let mut h = Self::start(address);
        h.until(|app| !app.files().is_empty(), "the folder listed");
        h
    }

    /// Started on `address`, without waiting for anything.
    pub fn start(address: &str) -> Self {
        guard();
        let ctx = egui::Context::default();
        let settings = Settings { check_updates: false, tips: false, local_permanent: true, servers_plain_warned: true, ..Settings::default() };
        let app = App::new(&ctx, settings, Some(address.to_string()));
        Harness { ctx, app, frame: eframe::Frame::_new_kittest(), full_asked: false, full_wanted: None, full: false, commands: Vec::new(), closing: false, two_passes: Vec::new() }
    }

    /// One frame, with `events` (keys) as the window would give them.
    pub fn frame(&mut self, events: Vec<egui::Event>) {
        let twice = events.iter().any(|e| matches!(e, egui::Event::Key { key, .. } if self.two_passes.contains(key)));
        let mut raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), events, max_texture_side: Some(16384), ..Default::default() };
        if self.full {
            raw.viewports.entry(egui::ViewportId::ROOT).or_default().fullscreen = Some(true);
        }
        if std::mem::take(&mut self.closing) {
            raw.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
        }
        eframe::App::raw_input_hook(&mut self.app, &self.ctx, &mut raw);
        let (app, frame) = (&mut self.app, &mut self.frame);
        let mut out = self.ctx.run_ui(raw, |ui| {
            if twice && ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("the test harness: a frame in two passes (#360)");
            }
            eframe::App::ui(app, ui, frame)
        });
        assert!(!twice || out.platform_output.num_completed_passes == 2, "egui ran the frame in {} passes, not two", out.platform_output.num_completed_passes);
        let full = |c: &egui::ViewportCommand| matches!(c, egui::ViewportCommand::Fullscreen(_));
        self.full_asked |= out.viewport_output.values().any(|v| v.commands.iter().any(full));
        let wanted = out.viewport_output.values().flat_map(|v| &v.commands).filter_map(|c| if let egui::ViewportCommand::Fullscreen(f) = c { Some(*f) } else { None }).next_back();
        self.full_wanted = wanted.or(self.full_wanted);
        self.commands = out.viewport_output.values().flat_map(|v| v.commands.iter().cloned()).collect();
        // No painter: the textures are dropped as handled.
        out.textures_delta.clear();
    }

    /// One frame in which the window's ✕ is pressed: whether the app lets the window close (it
    /// does when it does not answer `CancelClose`).
    pub fn close(&mut self) -> bool {
        self.closing = true;
        self.frame(Vec::new());
        !self.commands.contains(&egui::ViewportCommand::CancelClose)
    }

    /// Frames until `done`, 10 s at most.
    pub fn until(&mut self, done: impl Fn(&App) -> bool, what: &str) {
        let start = std::time::Instant::now();
        while !done(&self.app) {
            let a = &self.app;
            assert!(start.elapsed().as_secs() < 10, "not reached in 10 s: {what} (base {:?}, folder {:?}, error {:?}, address form {})", a.base, a.dir, a.error, a.open_address.is_some());
            self.frame(Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    pub fn key(&mut self, key: egui::Key) {
        self.key_with(key, egui::Modifiers::NONE);
    }

    pub fn key_with(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        let ev = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        self.frame(vec![ev(true), ev(false)]);
    }

    /// Nothing has the keyboard, as after a click on the body of a window.
    pub fn blur(&mut self) {
        self.ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }

    /// A widget has the keyboard, as the button that opened a question inside a form.
    pub fn focus_somewhere(&mut self) {
        self.ctx.memory_mut(|m| m.request_focus(egui::Id::unique("the button that opened it")));
    }
}

/// #311: a window with fields open over the grid (`open`), nothing focused (a click on its
/// body): the keys of the grid and the shortcuts reach nothing under it, the window stays alone.
pub(crate) fn keys_stay_in_window(h: &mut Harness, open: impl Fn(&App) -> bool) {
    use egui::{Key, Modifiers};
    let ctrl = Modifiers::COMMAND;
    (h.app.cursor, h.app.selected, h.app.focus) = (true, 0, crate::Focus::Grid);
    let keys = [(Key::ArrowRight, Modifiers::NONE), (Key::ArrowDown, Modifiers::NONE), (Key::End, Modifiers::NONE), (Key::Delete, Modifiers::NONE), (Key::F2, Modifiers::NONE), (Key::E, ctrl), (Key::N, ctrl), (Key::N, ctrl | Modifiers::SHIFT), (Key::B, ctrl), (Key::D, ctrl)];
    for (key, modifiers) in keys {
        h.blur();
        h.key_with(key, modifiers);
        h.frame(Vec::new());
        let a = &h.app;
        let forms = [a.rename.is_some(), a.new_folder.is_some(), a.new_image.is_some(), a.options].iter().filter(|o| **o).count();
        assert!(open(a) && forms == 1, "{key:?} {modifiers:?}: the window closed, or another came");
        assert!((a.cursor, a.selected) == (true, 0) && a.marked.is_empty(), "{key:?}: the grid moved under the window");
        assert!(a.delete_confirm.is_none() && a.editor.is_none() && a.batch.is_none() && a.duplicates.is_none() && a.viewing.is_none(), "{key:?} {modifiers:?}: reached the app under the window");
    }
}

/// The viewer open on b.png, the middle one of three (the arrows, Home and End would move it),
/// turned and not saved, for a window over it (#311).
pub(crate) fn turned_middle(name: &str) -> Harness {
    let mut h = Harness::new(&folder(name, &["a.png", "b.png", "c.png"]));
    h.app.activate(1);
    h.until(|app| app.zoom_of == Some(1), "b.png shown");
    h.app.act = Some(crate::actions::Act::Transform(crate::transform::Op::Cw, None));
    h.frame(Vec::new());
    assert!(h.app.turn_unsaved(), "b.png turned, not saved");
    h
}

/// #311: a window with fields open over the viewer (`open`), the keyboard on `focus` (`None`:
/// nothing, as after a click on its body; a text field; a button): the keys of the viewer and the
/// shortcuts reach nothing under it. The picture shown, its zoom, its turn not saved, full screen
/// stay as they were; no other window comes. Space and Enter press a button, Esc closes: not here.
pub(crate) fn keys_stay_over_viewer(h: &mut Harness, open: impl Fn(&App) -> bool, focus: Option<egui::Id>) {
    use egui::{Key, Modifiers};
    let (none, ctrl) = (Modifiers::NONE, Modifiers::COMMAND);
    let viewer = |a: &App| format!("{:?}", (a.viewing, a.zoom, a.zoom_of, a.pan, a.turn, a.turn_unsaved(), a.show.is_some()));
    let before = viewer(&h.app);
    assert!(h.app.viewing.is_some(), "the viewer is not open");
    let mut keys: Vec<(Key, Modifiers)> = [Key::ArrowRight, Key::ArrowLeft, Key::ArrowUp, Key::ArrowDown, Key::PageDown, Key::PageUp, Key::Home, Key::End, Key::Backspace, Key::Plus, Key::Minus, Key::Num0, Key::Num1, Key::Period, Key::Comma, Key::M, Key::S, Key::F11, Key::F5, Key::Delete, Key::F2].map(|k| (k, none)).to_vec();
    keys.extend([Key::R, Key::Z, Key::Y, Key::E, Key::S, Key::B, Key::D].map(|k| (k, ctrl)));
    keys.extend([(Key::R, ctrl | Modifiers::SHIFT), (Key::S, ctrl | Modifiers::SHIFT), (Key::Delete, Modifiers::SHIFT), (Key::Tab, none), (Key::Tab, Modifiers::SHIFT)]);
    for (key, modifiers) in keys {
        h.blur();
        if let Some(id) = focus {
            h.ctx.memory_mut(|m| m.request_focus(id));
        }
        h.key_with(key, modifiers);
        h.frame(Vec::new());
        let a = &h.app;
        assert!(open(a), "{key:?} {modifiers:?}: the window closed");
        assert_eq!(viewer(a), before, "{key:?} {modifiers:?}: the viewer under the window changed");
        assert!(!h.full_asked, "{key:?} {modifiers:?}: full screen asked under the window");
        assert!(a.delete_confirm.is_none() && a.editor.is_none() && a.batch.is_none() && a.duplicates.is_none() && a.save_ready.is_none() && a.convert.is_none(), "{key:?} {modifiers:?}: reached the app under the window");
        assert!(a.after_turn.is_none() && a.notice.is_none(), "{key:?} {modifiers:?}: a question came over the window");
    }
}

/// #311: Rename over the viewer (F2) keeps the keys of the viewer and the shortcuts, with the
/// keyboard nowhere, in the name field or on a button: the picture, its zoom, its turn not
/// saved and full screen stay as they were.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn rename_over_the_viewer_keeps_its_keys() {
    if own_folder("audit_family_1::rename_over_the_viewer_keeps_its_keys") {
        return;
    }
    let mut h = turned_middle("rename-viewer");
    h.app.ask_rename(None);
    h.frame(Vec::new());
    h.frame(Vec::new());
    let field = h.ctx.memory(|m| m.focused()).filter(|_| h.ctx.text_edit_focused()).expect("the name field has the keys");
    h.key(egui::Key::Tab);
    let button = h.ctx.memory(|m| m.focused()).filter(|f| *f != field && !h.ctx.text_edit_focused()).expect("Tab to a button");
    for focus in [None, Some(field), Some(button)] {
        keys_stay_over_viewer(&mut h, |a| a.rename.is_some(), focus);
    }
}

/// #311: a window that is not modal open over the grid (New image, Options): Tab, Shift+Tab and
/// the arrows, pressed many times, never take the keyboard out of it to the menus, the toolbar or
/// the grid under it (Enter then pressed what they had reached there).
pub(crate) fn focus_stays_in_window(h: &mut Harness) {
    use egui::{Key, Modifiers};
    let order = |h: &Harness| h.ctx.memory(|m| m.focused()).and_then(|id| h.ctx.read_response(id)).map(|r| r.layer_id.order);
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    // Options opens with nothing focused: a click on its first control.
    if h.ctx.memory(|m| m.focused()).is_none() {
        let first = h.ctx.viewport(|v| v.prev_pass.widgets.layers().filter(|(l, _)| l.order == egui::Order::Middle).find_map(|(_, w)| w.iter().find(|w| w.enabled && w.sense.is_focusable()).map(|w| w.id)));
        h.ctx.memory_mut(|m| m.request_focus(first.unwrap()));
        h.frame(Vec::new());
    }
    let none = Modifiers::NONE;
    for (key, modifiers) in [(Key::Tab, none), (Key::Tab, Modifiers::SHIFT), (Key::ArrowDown, none), (Key::ArrowRight, none), (Key::ArrowUp, none), (Key::ArrowLeft, none)] {
        let mut seen = std::collections::HashSet::new();
        // Tab: more presses than controls, to go past the last one (fewer seen than pressed).
        let times = if key == Key::Tab { 40 } else { 12 };
        for i in 1..=times {
            h.key_with(key, modifiers);
            h.frame(Vec::new());
            assert_eq!(order(h), Some(egui::Order::Middle), "{key:?} {modifiers:?} pressed {i} times: the keyboard left the window");
            seen.extend(h.ctx.memory(|m| m.focused()));
        }
        // Round all of them, not stuck on one.
        assert!(key != Key::Tab || (4..times).contains(&seen.len()), "{key:?} {modifiers:?} went through {} controls", seen.len());
    }
}

pub(crate) fn enter() -> egui::Key {
    egui::Key::Enter
}

/// Audit 1.6 (#196): a message that arrives while a deletion is being confirmed comes on top of
/// it, and Enter, pressed to close the message, confirmed the deletion under it instead: one
/// answer for all windows, taken by the first one drawn (`windows()`: the deletion before the
/// messages). Now the answer goes to the window on top only (`confirm::answer`).
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_1_6_enter_on_a_message_confirms_the_deletion_under_it() {
    if own_folder("audit_family_1::audit_1_6_enter_on_a_message_confirms_the_deletion_under_it") {
        return;
    }
    let (dir, mut h) = deletion_under_a_message("one-six");
    h.key(enter());
    settle(&mut h);
    let deleted = !dir.join("a.jpg").exists();
    eprintln!("after Enter: message open {}, deletion asked {}, a.jpg deleted {deleted}", h.app.notice.is_some(), h.app.delete_confirm.is_some());
    assert!(!deleted, "Enter on the message deleted a.jpg");
    assert!(h.app.notice.is_none() && h.app.delete_confirm.is_some(), "Enter should close the message and leave the deletion asked");
    // The deletion is on top now: the next keys are its own. Deleting for good opens on Cancel
    // (#311): Tab to Delete, then Enter.
    h.key(egui::Key::Tab);
    h.key(enter());
    settle(&mut h);
    assert!(!dir.join("a.jpg").exists() && h.app.delete_confirm.is_none(), "Enter on the deletion, now on top, did not delete a.jpg");
}

/// #196: Esc on the message closes the message only; the deletion under it stays asked, and Esc
/// again cancels it.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn esc_on_a_message_leaves_the_deletion_asked() {
    if own_folder("audit_family_1::esc_on_a_message_leaves_the_deletion_asked") {
        return;
    }
    let (dir, mut h) = deletion_under_a_message("esc-message");
    h.key(egui::Key::Escape);
    settle(&mut h);
    assert!(h.app.notice.is_none() && h.app.delete_confirm.is_some(), "Esc should close the message and leave the deletion asked");
    h.key(egui::Key::Escape);
    settle(&mut h);
    assert!(h.app.delete_confirm.is_none() && dir.join("a.jpg").exists(), "Esc on the deletion should cancel it");
}

/// A deletion of a.jpg asked (as Del), then a message on top of it, as from the background.
fn deletion_under_a_message(name: &str) -> (PathBuf, Harness) {
    let dir = folder(name, &["a.jpg", "b.jpg"]);
    let mut h = Harness::new(&dir);
    let a = h.app.files().iter().find(|f| f.path.ends_with("a.jpg")).unwrap().path.clone();
    h.app.ask_delete_paths(vec![a], false, None);
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    // A message from the background (a turn in series finished, a phone stalled…).
    h.app.tell("notice-close", "Something finished in the background.".into());
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let top = h.ctx.memory(|m| m.top_modal_layer());
    let notice = Some(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("notice")));
    eprintln!("top modal {top:?}, notice layer {notice:?}");
    assert!(top == notice, "the message is not the top window: then Enter answering the deletion would be right");
    (dir, h)
}

/// Some frames for what a key started (a deletion in the background).
fn settle(h: &mut Harness) {
    for _ in 0..10 {
        h.frame(Vec::new());
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// The viewer on a.png of the folder `name` (with b.png), turned and not saved.
fn turned_in_viewer(name: &str) -> (PathBuf, Harness) {
    let a = folder(name, &["a.png", "b.png"]);
    let mut h = Harness::new(&a);
    h.app.activate(0);
    h.until(|app| app.zoom_of == Some(0), "a.png shown");
    h.app.act = Some(crate::actions::Act::Transform(crate::transform::Op::Cw, None));
    h.frame(Vec::new());
    assert!(h.app.turn_unsaved(), "a.png turned, not saved");
    (a, h)
}

/// The path of the image in the viewer.
fn viewed(h: &Harness) -> Option<String> {
    h.app.viewing.and_then(|i| h.app.files().get(i)).map(|f| f.path.clone())
}

/// Save from the viewer, confirmed with Enter, until `file` is written turned (8 x 6 → 6 x 8).
fn save_turned(h: &mut Harness, file: &Path) {
    h.app.act = Some(crate::actions::Act::Save);
    h.until(|app| app.save_ready.is_some(), "the confirmation of Save");
    // With the original deleted for good the window opens on Cancel (#311): Tab to Save first.
    let path = h.app.save_ready.as_ref().map(|s| s.0.clone()).unwrap();
    let how = h.app.loader.as_ref().unwrap().deletion(&path, h.app.settings.local_permanent, h.app.settings.remote_delete);
    if how == crate::source::Deletion::Permanent {
        h.key(egui::Key::Tab);
    }
    h.key(enter());
    h.until(|app| app.saving.is_empty() && image::image_dimensions(file).is_ok_and(|d| d != (8, 6)), "the image written turned");
    h.until(|app| !app.turn_unsaved(), "the turn saved, not shown again over the image saved");
}

/// Audit 1.5: how the viewer was left with a.png turned and not saved. `leave` leaves it as the
/// user would; then the first image of what is shown opens, as a double click would.
/// Prints and returns: asked before leaving, the turn carried to another image.
fn audit_1_5_leave(name: &str, leave: impl FnOnce(&mut Harness, &Path)) -> (bool, Option<String>) {
    let (_, mut h) = turned_in_viewer(&format!("{name}-a"));
    let b = folder(&format!("{name}-b"), &["c.png", "d.png"]);
    leave(&mut h, &b);
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let asked = h.app.after_turn.is_some();
    let mut carried = None;
    if !asked {
        h.until(|app| !app.files().is_empty(), "a listing");
        h.app.activate(0);
        h.until(|app| app.zoom_of == Some(0), "the first image shown");
        let shown = h.app.files()[0].path.clone();
        if !shown.ends_with("a.png") && h.app.turn_unsaved() {
            carried = Some(shown);
        }
    }
    eprintln!("{name}: asked {asked}, turn carried to {carried:?}");
    (asked, carried)
}

/// Audit 1.5 (a): an address typed in the address bar with the viewer open (what Enter in the
/// bar does, `toolbar_end.rs`). Fixed (#198): `open_source` asks first.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_1_5_a_typed_address_drops_the_turn() {
    if own_folder("audit_family_1::audit_1_5_a_typed_address_drops_the_turn") {
        return;
    }
    let (asked, carried) = audit_1_5_leave("one-five-typed", |h, b| {
        let address = crate::navigation::typed_address(&format!("{}/", b.display()));
        h.app.open_source(&h.ctx.clone(), &address);
    });
    assert!(asked && carried.is_none(), "turn not asked about (carried to {carried:?})");
}

/// Audit 1.5 (b): View > Sort by from the viewer (here: name, descending) moved the turn of a.png
/// to b.png, sorted into its place, and Save wrote b.png turned. Fixed (#198): the viewer stays
/// on a.png with its turn, and Save writes a.png.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_1_5_b_sort_from_the_viewer_moves_the_turn() {
    if own_folder("audit_family_1::audit_1_5_b_sort_from_the_viewer_moves_the_turn") {
        return;
    }
    let (dir, mut h) = turned_in_viewer("one-five-sort");
    h.app.set_sort(crate::settings::SortBy::Name, true);
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let shown = viewed(&h);
    eprintln!("after the sort: viewer on {shown:?} (place {:?}, zoom of {:?}), turn not saved {}", h.app.viewing, h.app.zoom_of, h.app.turn_unsaved());
    assert!(shown.as_deref().is_some_and(|p| p.ends_with("a.png")), "the viewer left a.png: {shown:?}");
    assert!(h.app.turn_unsaved() && h.app.zoom_of == h.app.viewing, "the turn of a.png was dropped by the sort");
    save_turned(&mut h, &dir.join("a.png"));
    let b = image::image_dimensions(dir.join("b.png")).ok();
    eprintln!("a.png after Save: {:?}; b.png: {b:?} (both were 8 x 6)", image::image_dimensions(dir.join("a.png")).ok());
    assert_eq!(b, Some((8, 6)), "b.png, never turned, was written turned");
}

/// Audit 1.5 (c): an address handed over by another start of EyeCanSee (a double click in the
/// file manager), as `App::answer_handed` (`single.rs`) sends it. Fixed (#198): asked first.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_1_5_c_address_from_outside_drops_the_turn() {
    if own_folder("audit_family_1::audit_1_5_c_address_from_outside_drops_the_turn") {
        return;
    }
    let (asked, carried) = audit_1_5_leave("one-five-outside", |h, b| {
        h.app.tx.send(crate::loader::Msg::Open(format!("{}/", b.display()))).unwrap();
    });
    assert!(asked && carried.is_none(), "turn not asked about (carried to {carried:?})");
}

/// #198, the other side of the rename: the file shown is written again under the same name (a
/// save tried again from the files kept) and selected; its new listing must not keep the turn
/// over the new content, as it keeps it over a file renamed.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn a_file_shown_written_again_drops_the_turn() {
    if own_folder("audit_family_1::a_file_shown_written_again_drops_the_turn") {
        return;
    }
    let (dir, mut h) = turned_in_viewer("written-again");
    let a = viewed(&h).unwrap();
    image::DynamicImage::new_rgb8(30, 20).save(dir.join("a.png")).unwrap();
    h.app.select_after = Some(a);
    h.app.relist(h.app.dir.clone());
    h.until(|app| app.listing_pending.is_empty(), "the folder read again");
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    eprintln!("after a.png written again: viewer on {:?}, turn not saved {}", viewed(&h), h.app.turn_unsaved());
    assert!(!h.app.turn_unsaved(), "the turn stayed over a.png written again");
}

/// Audit 1.5 (d), #198: F2 in the viewer renames the image shown (a.png to z.png, which sorts it
/// after b.png). The viewer follows it with its turn not saved, and Save writes z.png turned.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_1_5_d_rename_keeps_the_turn() {
    if own_folder("audit_family_1::audit_1_5_d_rename_keeps_the_turn") {
        return;
    }
    let (dir, mut h) = turned_in_viewer("one-five-rename");
    let a = viewed(&h).unwrap();
    h.app.loader.as_ref().unwrap().rename(a, "z.png".into());
    h.until(|app| app.files().iter().any(|f| f.path.ends_with("z.png")) && app.listing_pending.is_empty(), "the folder read again");
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let shown = viewed(&h);
    eprintln!("after the rename: viewer on {shown:?} (place {:?}, zoom of {:?}), turn not saved {}", h.app.viewing, h.app.zoom_of, h.app.turn_unsaved());
    assert!(shown.as_deref().is_some_and(|p| p.ends_with("z.png")), "the viewer did not follow the image renamed: {shown:?}");
    assert!(h.app.turn_unsaved() && h.app.zoom_of == h.app.viewing, "the turn was dropped by the rename");
    save_turned(&mut h, &dir.join("z.png"));
    assert_eq!(image::image_dimensions(dir.join("b.png")).ok(), Some((8, 6)), "b.png, never turned, was written turned");
}

/// Audit 2.3: names that come from a server or a phone are not checked. A server lists, in a
/// folder that is pasted over a folder of the same name (Replace: the two merge), a file named
/// `../victim.txt`. Its destination is outside the folder pasted, and since a merge replaces the
/// files already there, the file of the computer with that name is written over with the
/// server's content. (For a file not there yet OpenDAL refuses the `..`.)
#[test]
fn audit_2_3_a_name_from_a_server_writes_outside_the_folder() {
    if own_folder("audit_family_1::audit_2_3_a_name_from_a_server_writes_outside_the_folder") {
        return;
    }
    use crate::source::Store;
    use crate::transfer::{Choice, Clip, Progress, run};
    let dir = guard().join("two-three");
    std::fs::create_dir_all(dir.join("Pictures/album")).unwrap();
    std::fs::write(dir.join("Pictures/album/a.jpg"), b"the computer's a.jpg").unwrap();
    std::fs::write(dir.join("Pictures/victim.txt"), b"the computer's file").unwrap();
    let mut tree = fake_ftp::Tree::default();
    tree.folder("/album", &[("a.jpg", b"the server's a.jpg"), ("../victim.txt", b"written by the server")]);
    let (target, tree) = fake_ftp::start(tree);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let from = Store::Ftp(crate::ftp::Server::shared(target));
    let to = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
    let clip = Clip { op: from, base: "ftp://tester@127.0.0.1/".into(), paths: vec!["album/".into()], cut: false, text: String::new() };
    let choices = HashMap::from([("album/".to_string(), Choice::Replace)]);
    let (_, r) = rt.block_on(run(&clip, &to, "/", "Pictures/", &choices, &Progress::default()));
    let after = std::fs::read_to_string(dir.join("Pictures/victim.txt")).unwrap();
    eprintln!("paste: {r:?}; Pictures/album/a.jpg: {:?}; Pictures/victim.txt: {after:?}", std::fs::read_to_string(dir.join("Pictures/album/a.jpg")));
    eprintln!("server saw: {:?}", tree.lock().unwrap().log);
    assert_eq!(after, "the computer's file", "a file outside the folder pasted was written over");
}

/// Audit 1.4: a deletion asked on one source and confirmed after an address from outside (a
/// double click in the file manager while EyeCanSee is open) opened another one. The confirmation
/// keeps the paths, relative to the source; it stays open, and the deletion goes to the source
/// open now. Here a server whose folder has the same path as one of the computer (a disk of
/// copies has the same folders): the computer's file goes.
///
/// Fixed (#197): another place closes the windows of the place left (`leave_source`).
/// Off Windows: there the computer's folders are not under "/".
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_1_4_deletion_goes_to_the_source_opened_meanwhile() {
    if own_folder("audit_family_1::audit_1_4_deletion_goes_to_the_source_opened_meanwhile") {
        return;
    }
    let (local, tree, mut h) = on_a_server_like_the_computer("one-four");
    let a = h.app.files()[0].path.clone();
    h.app.ask_delete_paths(vec![a.clone()], false, None);
    h.frame(Vec::new());
    h.app.tx.send(crate::loader::Msg::Open(format!("{}/", local.display()))).unwrap();
    h.until(|app| app.base == "/", "the computer's folder opened");
    eprintln!("deletion of {a:?} still asked on {:?}: {}", h.app.base, h.app.delete_confirm.is_some());
    assert!(h.app.delete_confirm.is_none(), "the deletion asked on the server is still open over the computer's folder");
    h.key(enter());
    for _ in 0..20 {
        h.frame(Vec::new());
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let gone = !local.join("a.png").exists();
    let server_deleted = tree.lock().unwrap().log.iter().any(|l| l.starts_with("DELE") || l.starts_with("RNFR"));
    eprintln!("after Enter: the computer's a.png gone {gone}; the server asked to delete or move {server_deleted}; {:?}", std::fs::read_dir(&local).unwrap().flatten().map(|e| e.file_name()).collect::<Vec<_>>());
    assert!(!gone, "the computer's a.png went: the deletion was asked for the server's");
    assert!(!server_deleted, "the server's a.png went: the question should have closed");
}

/// A server whose folder has the same path as `name`, a folder of the computer with a.png in it:
/// the app opened on the server's.
fn on_a_server_like_the_computer(name: &str) -> (PathBuf, std::sync::Arc<std::sync::Mutex<fake_ftp::Tree>>, Harness) {
    let local = folder(name, &["a.png"]);
    let png = std::fs::read(local.join("a.png")).unwrap();
    let abs = local.display().to_string();
    let mut tree = fake_ftp::Tree::default();
    tree.folder(&abs, &[("a.png", &png)]);
    let (target, tree) = fake_ftp::start(tree);
    let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Harness::open(&format!("ftp://tester:x@127.0.0.1:{}{abs}/", target.port))));
    let Ok(h) = opened else { panic!("server saw {:?}", tree.lock().unwrap().log) };
    assert!(h.app.base.starts_with("ftp://"), "on the server: {}", h.app.base);
    (local, tree, h)
}

/// #197, the other windows: the duplicates closes (its search stops) and Save in another format
/// stays on the server's image, in the server's place, when the computer's folder opens.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn another_place_closes_the_windows_of_the_place_left() {
    if own_folder("audit_family_1::another_place_closes_the_windows_of_the_place_left") {
        return;
    }
    let (local, _tree, mut h) = on_a_server_like_the_computer("another-place");
    let a = h.app.files()[0].path.clone();
    h.app.open_duplicates(None);
    h.app.open_convert(Some(a));
    h.frame(Vec::new());
    assert!(h.app.duplicates.is_some() && h.app.convert.is_some(), "the duplicates and Save in another format open");
    h.app.tx.send(crate::loader::Msg::Open(format!("{}/", local.display()))).unwrap();
    h.until(|app| app.base == "/", "the computer's folder opened");
    assert!(h.app.duplicates.is_none(), "the duplicates of the server stayed over the computer's folder");
    let place = h.app.convert.as_ref().and_then(|w| w.place.as_ref()).map(|p| p.base.clone());
    assert!(place.as_deref().is_some_and(|b| b.starts_with("ftp://")), "Save in another format not kept on the server: {place:?}");
}

/// Audit 2.1 on a real phone (`EYECANSEE_MTP=<serial>`): a folder of our own, made by the test in
/// `Pictures/` (`EyeCanSee-audit-f1`, four files of 4 MB), moved to this computer; once the move
/// has looked through it, a photo arrives in it (written through the same connection, as the
/// camera would). The folder must not be deleted with that photo in it. The test makes and
/// removes only its own folder; it stops if the folder is already there. Passed on the POCO X6 Pro
/// with the fix of #194 (2026-11-21: move 0.69 s, the photo written 18 ms after the copy began
/// stayed in its folder).
#[test]
#[ignore = "audit 2.1 (#194): needs the phone, EYECANSEE_MTP=<serial>"]
fn audit_2_1_phone_move_deletes_a_photo_taken_meanwhile() {
    use crate::source::Store;
    use crate::transfer::{Clip, Progress, run};
    let run_dir = guard();
    let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let phone = Store::Mtp(crate::mtp::Phone::shared(&serial));
    let pc_dir = run_dir.join("pc");
    std::fs::create_dir_all(&pc_dir).unwrap();
    let pc = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(pc_dir.to_str().unwrap())).unwrap());
    rt.block_on(async {
        let storage = phone.list("").await.unwrap().dirs[0].clone();
        let pictures = format!("{storage}Pictures/");
        assert!(!phone.names(&pictures).await.unwrap().contains("EyeCanSee-audit-f1"), "the test folder is already on the phone: left as it is");
        let folder = format!("{pictures}EyeCanSee-audit-f1/");
        phone.create_dir(&folder).await.unwrap();
        for i in 0..4u8 {
            crate::replace::write_whole(&phone, &format!("{folder}IMG_{i}.jpg"), vec![i; 4 << 20]).await.unwrap();
        }
        let clip = Clip { op: phone.clone(), base: format!("mtp://{serial}/"), paths: vec![folder.clone()], cut: true, text: String::new() };
        let p = Progress::default();
        let late = async {
            while p.started.get().is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let at = std::time::Instant::now();
            let r = crate::replace::write_whole(&phone, &format!("{folder}IMG_late.jpg"), b"taken while moving".to_vec()).await;
            (r, at.elapsed())
        };
        let t = std::time::Instant::now();
        let none = std::collections::HashMap::new();
        let ((_, moved), (written, waited)) = tokio::join!(run(&clip, &pc, "/", "", &none, &p), late);
        let still = phone.names(&pictures).await.unwrap().contains("EyeCanSee-audit-f1");
        let on_phone = if still { phone.names(&folder).await.unwrap_or_default() } else { Default::default() };
        let copied: Vec<String> = std::fs::read_dir(pc_dir.join("EyeCanSee-audit-f1")).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        eprintln!("move in {:.1?}: {moved:?}; late photo written {written:?} after {waited:.1?}; folder on the phone {still} {on_phone:?}; copied {copied:?}", t.elapsed());
        // Our folder goes in any case.
        if still {
            let _ = phone.delete(&folder, crate::source::Deletion::Permanent).await;
        }
        assert!(written.is_ok(), "race lost: the late photo was not written (run again)");
        assert!(on_phone.contains("IMG_late.jpg") || copied.iter().any(|n| n == "IMG_late.jpg"), "the photo taken during the move is gone");
    });
}

/// Measure on a real phone (`EYECANSEE_MTP=<serial>`), after the #194 fix: a folder of our own,
/// `Pictures/prove-078-misura` (300 files of 250 kB, as a folder of small photos), moved to this
/// computer: how long the copy takes, and how long the originals then take to go. Then the folder
/// is written again and moved with a photo arriving in it: the photo stays, with its folder. The
/// test makes and removes only its own folder, whatever happens; it stops if the folder is already
/// there. Run it with a long timeout (`run.sh <name> 600 0`): an upload cut short blocks the phone.
#[test]
#[ignore = "measure (#194): needs the phone, EYECANSEE_MTP=<serial>"]
fn phone_move_of_300_files_timed() {
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    use std::time::{Duration, Instant};

    use crate::source::{Deletion, Store};
    use crate::transfer::{Clip, Progress, run};
    const FILES: usize = 300;
    let run_dir = guard();
    let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let phone = Store::Mtp(crate::mtp::Phone::shared(&serial));
    let pc_dir = run_dir.join("pc");
    let pc_store = |dir: &Path| {
        std::fs::create_dir_all(dir).unwrap();
        Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap())
    };
    let pc = pc_store(&pc_dir);
    rt.block_on(async {
        let storage = phone.list("").await.unwrap().dirs[0].clone();
        let pictures = format!("{storage}Pictures/");
        assert!(!phone.names(&pictures).await.unwrap().contains("prove-078-misura"), "the test folder is already on the phone: left as it is");
        let folder = format!("{pictures}prove-078-misura/");
        let fill = async || -> Result<Duration, crate::source::Error> {
            let t = Instant::now();
            phone.create_dir(&folder).await?;
            for i in 0..FILES {
                crate::replace::write_whole(&phone, &format!("{folder}IMG_{i:03}.jpg"), vec![i as u8; 250_000]).await?;
            }
            Ok(t.elapsed())
        };
        let measured = async {
            let written = fill().await?;
            let clip = Clip { op: phone.clone(), base: format!("mtp://{serial}/"), paths: vec![folder.clone()], cut: true, text: String::new() };
            let p = Progress::default();
            let ended = AtomicBool::new(false);
            // When the last file was copied: the originals go after that.
            let watch = async {
                while !ended.load(Relaxed) && (p.started.get().is_none() || p.files.load(Relaxed) < p.files_total.load(Relaxed)) {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                Instant::now()
            };
            let moving = async {
                let none = std::collections::HashMap::new();
                let r = run(&clip, &pc, "/", "", &none, &p).await;
                ended.store(true, Relaxed);
                r
            };
            let t = Instant::now();
            let ((_, moved), copied_at) = tokio::join!(moving, watch);
            let end = Instant::now();
            let started = p.started.get().copied().unwrap_or(end);
            let still = phone.names(&pictures).await?.contains("prove-078-misura");
            let copied = std::fs::read_dir(pc_dir.join("prove-078-misura")).map(|d| d.count()).unwrap_or(0);
            eprintln!(
                "written in {written:.1?}; move {moved:?} in {:.2?}: looking {:.2?}, copy {:.2?}, originals {:.2?}; folder still on the phone: {still}; {copied} files copied",
                end - t,
                started.saturating_duration_since(t),
                copied_at.saturating_duration_since(started),
                end.saturating_duration_since(copied_at)
            );
            // Again, with a photo arriving in the folder once the copy started: it stays on the
            // phone, with its folder, and what was copied goes file by file.
            let late_kept = !still && {
                let written = fill().await?;
                eprintln!("written again in {written:.1?}");
                moved_with_a_late_photo(&clip, &pc_store(&run_dir.join("pc2"))).await
            };
            Ok::<_, crate::source::Error>(moved.is_ok() && copied == FILES && !still && late_kept)
        };
        let ok = measured.await;
        // Our folder goes in any case.
        if phone.names(&pictures).await.map(|n| n.contains("prove-078-misura")).unwrap_or(true) {
            let r = phone.delete(&folder, Deletion::Permanent).await;
            eprintln!("test folder removed at the end: {r:?}");
        }
        assert!(!phone.names(&pictures).await.unwrap().contains("prove-078-misura"), "the test folder is still on the phone");
        assert_eq!(ok, Ok(true), "the move did not go as it should");
    });
}

/// The folder of `clip` moved to `pc` with a photo arriving in it once the copy started (in
/// `phone_move_of_300_files_timed`): whether the photo stayed on the phone, alone in its folder.
async fn moved_with_a_late_photo(clip: &crate::transfer::Clip, pc: &crate::source::Store) -> bool {
    use std::time::{Duration, Instant};
    let p = crate::transfer::Progress::default();
    let folder = &clip.paths[0];
    let late = async {
        while p.started.get().is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        crate::replace::write_whole(&clip.op, &format!("{folder}IMG_late.jpg"), b"taken while moving".to_vec()).await
    };
    let t = Instant::now();
    let none = std::collections::HashMap::new();
    let ((_, moved), wrote) = tokio::join!(crate::transfer::run(clip, pc, "/", "", &none, &p), late);
    let on_phone = clip.op.names(folder).await.unwrap_or_default();
    eprintln!("with a photo arriving: move {moved:?} in {:.2?}, photo written {wrote:?}; on the phone {on_phone:?}; named {:?}", t.elapsed(), p.left.lock().unwrap());
    wrote.is_ok() && on_phone.len() == 1 && on_phone.contains("IMG_late.jpg")
}

/// A folder for the tests of 2.4 and 2.5, which need no app: under the system's temporary
/// folder, new for every run. The two tests fail on Windows (NTFS) and pass elsewhere: on Linux
/// they check the rest of the path (the fake server, the names, the paste).
fn windows_folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eyecansee-audit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The names in `dir` with their sizes, as Explorer shows them.
fn shown(dir: &Path) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = std::fs::read_dir(dir).unwrap().flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().map(|m| m.len()).unwrap_or(0))).collect();
    v.sort();
    v
}

/// Audit 2.4 (Windows): a file named `Mare 12:30.jpg`, a valid name on a server or a phone, moved
/// from a server to a folder of Windows. On NTFS the colon opens the hidden stream `30.jpg` of an
/// empty file `Mare 12`: the copy "succeeds", the folder shows a file of 0 bytes, and the move
/// deletes the original on the server.
#[test]
fn audit_2_4_windows_colon_in_a_name_from_a_server() {
    use crate::source::Store;
    use crate::transfer::{Clip, Progress, run};
    let dir = windows_folder("2-4");
    let content = vec![7u8; 1000];
    let mut tree = fake_ftp::Tree::default();
    tree.folder("/album", &[("Mare 12:30.jpg", &content)]);
    let (target, tree) = fake_ftp::start(tree);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let from = Store::Ftp(crate::ftp::Server::shared(target));
    let to = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
    let clip = Clip { op: from, base: "ftp://tester@127.0.0.1/".into(), paths: vec!["album/Mare 12:30.jpg".into()], cut: true, text: String::new() };
    let (gone, r) = rt.block_on(run(&clip, &to, "C:\\", "", &HashMap::new(), &Progress::default()));
    let on_server = tree.lock().unwrap().files.contains_key("/album/Mare 12:30.jpg");
    let visible = shown(&dir);
    eprintln!("move: {r:?}, gone from the server {gone:?} (still there: {on_server}); the folder shows {visible:?}");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(on_server || visible.iter().any(|(_, size)| *size == content.len() as u64), "the photo is neither on the server nor visible in the folder");
}

/// Audit 2.5 (Windows): a file pasted into a folder that has the same name with other capitals
/// (`IMG_001.JPG` over `img_001.jpg`). For EyeCanSee the name is free, so it asks nothing; for
/// Windows it is the same file, written over. Fixed (#204): the name counts as taken where the
/// destination ignores capitals, and the app asks.
#[test]
fn audit_2_5_windows_capitals_overwrite_without_asking() {
    use crate::source::Store;
    use crate::transfer::{Clip, Progress, conflicts, run};
    let dir = windows_folder("2-5");
    std::fs::create_dir_all(dir.join("camera")).unwrap();
    std::fs::create_dir_all(dir.join("photos")).unwrap();
    std::fs::write(dir.join("camera/IMG_001.JPG"), b"the new photo").unwrap();
    std::fs::write(dir.join("photos/img_001.jpg"), b"the photo already there").unwrap();
    let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
    let base = "C:\\";
    let rt = tokio::runtime::Runtime::new().unwrap();
    let clip = Clip { op: op.clone(), base: base.into(), paths: vec!["camera/IMG_001.JPG".into()], cut: false, text: String::new() };
    let asked = rt.block_on(conflicts(&clip, &op, base, "photos/")).unwrap();
    // As the app: nothing asked, nothing answered.
    let (_, r) = rt.block_on(run(&clip, &op, base, "photos/", &HashMap::new(), &Progress::default()));
    let visible = shown(&dir.join("photos"));
    let there = std::fs::read_to_string(dir.join("photos/img_001.jpg")).unwrap_or_default();
    eprintln!("asked about {asked:?}; paste {r:?}; photos shows {visible:?}; img_001.jpg holds {there:?}");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!asked.is_empty() || there == "the photo already there", "the photo already there was written over without a question");
}
