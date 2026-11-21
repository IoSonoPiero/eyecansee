//! The app without a screen, for the tests that need the whole app (windows, keys, sources):
//! the part of `audit_family_1` on `main` that the tests of the guide over a window use (#196).
//! A test runs again in a child process with settings, cache, data (trash) and temporary files
//! in a folder of its own and `EYECANSEE_MULTI=1` (`own_folder`), never the user's nor the copy
//! open.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::App;
use crate::settings::Settings;

/// The variables that `own_folder` points into the folder of a run: settings, cache, data (the
/// system trash), temporary files.
const RUN_VARS: &[(&str, &str)] = &[("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data"), ("TMPDIR", "tmp"), ("APPDATA", "config"), ("LOCALAPPDATA", "cache"), ("TEMP", "tmp"), ("TMP", "tmp")];

/// The folder of this run (`AUDIT_ENV`, made by `own_folder`): the test stops at once if settings,
/// cache or data could be the user's, or if it could talk to the copy of EyeCanSee open.
pub(crate) fn guard() -> PathBuf {
    let env = |v: &str| std::env::var(v).unwrap_or_default();
    let run = env("AUDIT_ENV");
    assert!(!run.is_empty() && run.contains("famiglia1-tools"), "run through own_folder: AUDIT_ENV={run:?}");
    let vars: &[&str] = if cfg!(windows) { &["APPDATA", "LOCALAPPDATA", "TEMP"] } else { &["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "TMPDIR"] };
    for v in vars {
        assert!(env(v).starts_with(&run), "{v} not in the folder of this run: {:?}", env(v));
    }
    assert_eq!(env("EYECANSEE_MULTI"), "1", "the single instance would reach the user's copy");
    PathBuf::from(run).canonicalize().unwrap()
}

/// Unless the test already runs in a folder of its own (this child), the test `name` (its path,
/// as `cargo test` lists it) runs again in a child process of this test binary, with settings,
/// cache, data and temporary files in a new folder deleted at the end, and `EYECANSEE_MULTI=1`.
/// `true` in the parent once the child passed: the test returns. A child that fails fails the test.
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
}

impl Harness {
    pub fn new(dir: &Path) -> Self {
        guard();
        let ctx = egui::Context::default();
        let settings = Settings { check_updates: false, tips: false, local_permanent: true, servers_plain_warned: true, ..Settings::default() };
        let app = App::new(&ctx, settings, Some(format!("{}/", dir.display())));
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

    /// A key pressed and let go, in one frame.
    pub fn key(&mut self, key: egui::Key) {
        let ev = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        self.frame(vec![ev(true), ev(false)]);
    }
}
