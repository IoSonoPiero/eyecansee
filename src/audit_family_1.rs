//! Audit of 2026-11-17, family 1 (files of the user that can be lost or overwritten): tests that
//! reproduce the points that need the whole app (windows, sources, the viewer), run without a
//! screen. Ignored until each point is fixed; run them one at a time with
//! `notes/lavori/audit-famiglia-1/script/run.sh`, which gives every run its own settings, cache,
//! data folder (trash) and `EYECANSEE_MULTI=1` (never the instance the user has open).

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
    PathBuf::from(run)
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
        guard();
        let ctx = egui::Context::default();
        let settings = Settings { check_updates: false, tips: false, local_permanent: true, ..Settings::default() };
        let app = App::new(&ctx, settings, Some(format!("{}/", dir.display())));
        let mut h = Harness { ctx, app, frame: eframe::Frame::_new_kittest() };
        h.until(|app| !app.files().is_empty(), "the folder listed");
        h
    }

    /// One frame, with `events` (keys) as the window would give them.
    pub fn frame(&mut self, events: Vec<egui::Event>) {
        let mut raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), events, ..Default::default() };
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
            assert!(start.elapsed().as_secs() < 10, "not reached in 10 s: {what}");
            self.frame(Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    pub fn key(&mut self, key: egui::Key) {
        let ev = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
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
