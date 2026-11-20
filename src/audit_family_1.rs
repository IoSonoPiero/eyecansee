//! Audit of 2026-11-17, family 1 (files of the user that can be lost or overwritten): tests that
//! reproduce the points that need the whole app (windows, sources, the viewer), run without a
//! screen. Ignored until each point is fixed; run them one at a time with
//! `notes/lavori/audit-famiglia-1/script/run.sh`, which gives every run its own settings, cache,
//! data folder (trash) and `EYECANSEE_MULTI=1` (never the instance the user has open).

mod fake_ftp;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::App;
use crate::settings::Settings;

/// The folder of this run (`AUDIT_ENV`, made by `run.sh`): the test stops at once if settings,
/// cache or data could be the user's, or if it could talk to the copy of EyeCanSee open.
pub(crate) fn guard() -> PathBuf {
    let env = |v: &str| std::env::var(v).unwrap_or_default();
    let run = env("AUDIT_ENV");
    assert!(!run.is_empty() && run.contains("famiglia1-tools"), "run through run.sh: AUDIT_ENV={run:?}");
    for v in ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "TMPDIR"] {
        assert!(env(v).starts_with(&run), "{v} not in the folder of this run: {:?}", env(v));
    }
    assert_eq!(env("EYECANSEE_MULTI"), "1", "the single instance would reach the user's copy");
    PathBuf::from(run).canonicalize().unwrap()
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
}

impl Harness {
    pub fn new(dir: &Path) -> Self {
        Self::open(&format!("{}/", dir.display()))
    }

    /// Started on `address`, a folder.
    pub fn open(address: &str) -> Self {
        guard();
        let ctx = egui::Context::default();
        let settings = Settings { check_updates: false, tips: false, local_permanent: true, servers_plain_warned: true, ..Settings::default() };
        let app = App::new(&ctx, settings, Some(address.to_string()));
        let mut h = Harness { ctx, app, frame: eframe::Frame::_new_kittest() };
        h.until(|app| !app.files().is_empty(), "the folder listed");
        h
    }

    /// One frame, with `events` (keys) as the window would give them.
    pub fn frame(&mut self, events: Vec<egui::Event>) {
        let mut raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), events, max_texture_side: Some(16384), ..Default::default() };
        eframe::App::raw_input_hook(&mut self.app, &self.ctx, &mut raw);
        let (app, frame) = (&mut self.app, &mut self.frame);
        let mut out = self.ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, frame));
        // No painter: the textures are dropped as handled.
        out.textures_delta.clear();
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
}

pub(crate) fn enter() -> egui::Key {
    egui::Key::Enter
}

/// Audit 1.6: a message that arrives while a deletion is being confirmed comes on top of it, and
/// Enter, pressed to close the message, confirms the deletion under it instead. One answer for
/// all windows (`delete_answer`), taken by the first one drawn (`windows()`: the deletion before
/// the messages).
#[test]
#[ignore = "audit 1.6: not fixed yet"]
fn audit_1_6_enter_on_a_message_confirms_the_deletion_under_it() {
    let dir = folder("one-six", &["a.jpg", "b.jpg"]);
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
    assert!(notice.is_some() && top == notice, "the message is not the top window: then Enter answering the deletion would be right");
    h.key(enter());
    for _ in 0..10 {
        h.frame(Vec::new());
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let deleted = !dir.join("a.jpg").exists();
    eprintln!("after Enter: message open {}, deletion asked {}, a.jpg deleted {deleted}", h.app.notice.is_some(), h.app.delete_confirm.is_some());
    assert!(!deleted, "Enter on the message deleted a.jpg");
    assert!(h.app.notice.is_none() && h.app.delete_confirm.is_some(), "Enter should close the message and leave the deletion asked");
}

/// Audit 1.5: how the viewer was left with a.png turned and not saved. `leave` leaves it as the
/// user would; then the first image of what is shown opens, as a double click would.
/// Prints and returns: asked before leaving, the turn lost, the turn carried to another image.
fn audit_1_5_leave(name: &str, leave: impl FnOnce(&mut Harness, &Path)) -> (bool, bool, Option<String>) {
    audit_1_5_leave_then(name, leave, |_, _| {})
}

/// `audit_1_5_leave`, then `after` with the folder of a.png when the turn was carried.
fn audit_1_5_leave_then(name: &str, leave: impl FnOnce(&mut Harness, &Path), after: impl FnOnce(&mut Harness, &Path)) -> (bool, bool, Option<String>) {
    let a = folder(&format!("{name}-a"), &["a.png", "b.png"]);
    let b = folder(&format!("{name}-b"), &["c.png", "d.png"]);
    let mut h = Harness::new(&a);
    h.app.activate(0);
    h.until(|app| app.zoom_of == Some(0), "a.png shown");
    h.app.act = Some(crate::actions::Act::Transform(crate::transform::Op::Cw, None));
    h.frame(Vec::new());
    assert!(h.app.turn_unsaved(), "a.png turned, not saved");
    leave(&mut h, &b);
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let asked = h.app.after_turn.is_some();
    let lost = !asked && h.app.viewing.is_none() && h.app.turn.is_none();
    let mut carried = None;
    if !asked {
        h.until(|app| !app.files().is_empty(), "a listing");
        h.app.activate(0);
        h.until(|app| app.zoom_of == Some(0), "the first image shown");
        let shown = h.app.files()[0].path.clone();
        if !shown.ends_with("a.png") && h.app.turn_unsaved() {
            carried = Some(shown);
            after(&mut h, &a);
        }
    }
    eprintln!("{name}: asked {asked}, turn lost {lost}, turn carried to {carried:?}");
    (asked, lost, carried)
}

/// Audit 1.5 (a): an address typed in the address bar with the viewer open (what Enter in the
/// bar does, `toolbar_end.rs`).
#[test]
#[ignore = "audit 1.5: not fixed yet"]
fn audit_1_5_a_typed_address_drops_the_turn() {
    let (asked, _, carried) = audit_1_5_leave("one-five-typed", |h, b| {
        let address = crate::navigation::typed_address(&format!("{}/", b.display()));
        h.app.open_source(&h.ctx.clone(), &address);
    });
    assert!(asked && carried.is_none(), "turn not asked about (carried to {carried:?})");
}

/// Audit 1.5 (b): View > Sort by from the viewer (here: name, descending).
#[test]
#[ignore = "audit 1.5: not fixed yet"]
fn audit_1_5_b_sort_from_the_viewer_moves_the_turn() {
    let mut saved = None;
    let (asked, _, carried) = audit_1_5_leave_then("one-five-sort", |h, _| h.app.set_sort(crate::settings::SortBy::Name, true), |h, dir| {
        // Save, confirmed with Enter: b.png, never turned, is written turned.
        h.app.act = Some(crate::actions::Act::Save);
        h.until(|app| app.save_ready.is_some(), "the confirmation of Save");
        h.key(enter());
        let file = dir.join("b.png");
        h.until(|app| app.saving.is_empty() && image::image_dimensions(&file).is_ok_and(|d| d != (8, 6)), "b.png written turned");
        saved = image::image_dimensions(&file).ok();
    });
    eprintln!("b.png after Save: {saved:?} (it was 8 x 6)");
    assert!(asked && carried.is_none(), "turn not asked about (carried to {carried:?}, saved as {saved:?})");
}

/// Audit 1.5 (c): an address handed over by another start of EyeCanSee (a double click in the
/// file manager), as `single::listen` sends it.
#[test]
#[ignore = "audit 1.5: not fixed yet"]
fn audit_1_5_c_address_from_outside_drops_the_turn() {
    let (asked, _, carried) = audit_1_5_leave("one-five-outside", |h, b| {
        h.app.tx.send(crate::loader::Msg::Open(format!("{}/", b.display()))).unwrap();
    });
    assert!(asked && carried.is_none(), "turn not asked about (carried to {carried:?})");
}

/// Audit 2.3: names that come from a server or a phone are not checked. A server lists, in a
/// folder that is pasted over a folder of the same name (Replace: the two merge), a file named
/// `../victim.txt`. Its destination is outside the folder pasted, and since a merge replaces the
/// files already there, the file of the computer with that name is written over with the
/// server's content. (For a file not there yet OpenDAL refuses the `..`.)
#[test]
#[ignore = "audit 2.3: not fixed yet"]
fn audit_2_3_a_name_from_a_server_writes_outside_the_folder() {
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
#[test]
#[ignore = "audit 1.4: not fixed yet"]
fn audit_1_4_deletion_goes_to_the_source_opened_meanwhile() {
    let local = folder("one-four", &["a.png"]);
    let png = std::fs::read(local.join("a.png")).unwrap();
    let abs = local.display().to_string();
    let mut tree = fake_ftp::Tree::default();
    tree.folder(&abs, &[("a.png", &png)]);
    let (target, tree) = fake_ftp::start(tree);
    let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Harness::open(&format!("ftp://tester:x@127.0.0.1:{}{abs}/", target.port))));
    let Ok(mut h) = opened else { panic!("server saw {:?}", tree.lock().unwrap().log) };
    assert!(h.app.base.starts_with("ftp://"), "on the server: {}", h.app.base);
    let a = h.app.files()[0].path.clone();
    h.app.ask_delete_paths(vec![a.clone()], false, None);
    h.frame(Vec::new());
    h.app.tx.send(crate::loader::Msg::Open(format!("{abs}/"))).unwrap();
    h.until(|app| app.base == "/", "the computer's folder opened");
    eprintln!("deletion of {a:?} still asked on {:?}: {}", h.app.base, h.app.delete_confirm.is_some());
    h.key(enter());
    for _ in 0..20 {
        h.frame(Vec::new());
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let gone = !local.join("a.png").exists();
    let server_deleted = tree.lock().unwrap().log.iter().any(|l| l.starts_with("DELE") || l.starts_with("RNFR"));
    eprintln!("after Enter: the computer's a.png gone {gone}; the server asked to delete or move {server_deleted}; {:?}", std::fs::read_dir(&local).unwrap().flatten().map(|e| e.file_name()).collect::<Vec<_>>());
    assert!(!gone, "the computer's a.png went: the deletion was asked for the server's");
}

/// Audit 2.1 on a real phone (`EYECANSEE_MTP=<serial>`): a folder of our own, made by the test in
/// `Pictures/` (`EyeCanSee-audit-f1`, four files of 4 MB), moved to this computer; once the move
/// has looked through it, a photo arrives in it (written through the same connection, as the
/// camera would). The folder must not be deleted with that photo in it. The test makes and
/// removes only its own folder; it stops if the folder is already there.
#[test]
#[ignore = "audit 2.1: not fixed yet, needs the phone"]
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
