//! A start with Vulkan that never drew (Windows): a broken Vulkan driver can bring the program
//! down inside its own code before the first frame, where no code of ours can catch it (Intel's
//! `igvk64.dll`, Store user report of 20/11). Before the window is made with Vulkan a mark is
//! written in the configuration folder (`vulkan-start`), and taken away once the first frame has
//! been drawn. A mark found at the next start means the last one with Vulkan ended before that:
//! this one goes back to DirectX 12, puts it back in the Options and says why (`App::new`).
//! Closing the program while it starts, or a second start while the first one is still slow to
//! draw, does the same: harmless, DirectX 12 is the default.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;

use crate::settings::{Graphics, Settings};

/// The mark's file, beside `settings.json` (the test copy has its own folder).
const MARK: &str = "vulkan-start";

/// This start wrote the mark: taken away after the first frame (`frame_drawn`).
static MARKED: AtomicBool = AtomicBool::new(false);
/// This start went back to DirectX 12 after a start with Vulkan that never drew.
static FELL_BACK: AtomicBool = AtomicBool::new(false);

/// What this start does with the graphics card.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The mark is written before the window: Vulkan may be used.
    pub(crate) mark: bool,
    /// The last start with Vulkan ended before its first frame: back to DirectX 12.
    pub(crate) fell_back: bool,
}

/// The plan for a start: on Windows only, where the choice exists. `marked_before`: the mark of
/// an earlier start is still there.
pub(crate) fn plan(windows: bool, graphics: Graphics, marked_before: bool) -> Plan {
    if !windows {
        return Plan::default();
    }
    let fell_back = marked_before && graphics == Graphics::Vulkan;
    Plan { mark: graphics == Graphics::Vulkan && !fell_back, fell_back }
}

/// Carries out the plan on the mark at `path` and on the settings (saved by the caller): an old
/// mark is always taken away, a new one written when Vulkan may be used.
pub(crate) fn apply(path: &Path, plan: &Plan, settings: &mut Settings) {
    if plan.fell_back {
        settings.graphics = Graphics::Directx12;
    }
    if plan.mark {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format!("EyeCanSee {} starting with Vulkan\n", crate::version()));
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Before the window is made: the mark checked, written or taken away, and the settings changed
/// and saved when this start goes back to DirectX 12. Nothing outside Windows.
pub(crate) fn before_window(settings: &mut Settings) {
    if !cfg!(windows) {
        return;
    }
    let Some(path) = crate::settings::config_path(MARK) else { return };
    let plan = plan(true, settings.graphics, path.exists());
    apply(&path, &plan, settings);
    if plan.fell_back {
        settings.save();
    }
    log!("graphics: {:?}, mark {}, back to DirectX 12 {}", settings.graphics, plan.mark, plan.fell_back);
    MARKED.store(plan.mark, Ordering::Relaxed);
    FELL_BACK.store(plan.fell_back, Ordering::Relaxed);
}

/// This start went back to DirectX 12: the user is told why (`App::new`).
pub(crate) fn fell_back() -> bool {
    FELL_BACK.load(Ordering::Relaxed)
}

/// At every frame: the mark goes away once the first frame has been drawn and shown (`frames`
/// counts the frames completed: eframe paints each one right after `App::ui`). Until then a
/// repaint is asked, so it does not wait for the mouse.
pub(crate) fn frame_drawn(ctx: &egui::Context) {
    if !MARKED.load(Ordering::Relaxed) {
        return;
    }
    let Some(path) = crate::settings::config_path(MARK) else { return };
    if take_after_first(&path, ctx.cumulative_frame_nr()) {
        MARKED.store(false, Ordering::Relaxed);
        log!("graphics: first frame drawn, mark taken away");
    } else {
        ctx.request_repaint();
    }
}

/// The mark at `path` taken away once `frames` (completed) is at least one; true when done.
fn take_after_first(path: &Path, frames: u64) -> bool {
    if frames == 0 {
        return false;
    }
    let _ = std::fs::remove_file(path);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-gpu-start-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("eyecansee").join(MARK)
    }

    #[test]
    fn only_on_windows() {
        for g in [Graphics::Directx12, Graphics::Vulkan] {
            for m in [false, true] {
                assert_eq!(plan(false, g, m), Plan::default());
            }
        }
    }

    #[test]
    fn mark_written_with_vulkan_only() {
        let path = temp("written");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        let p = plan(true, s.graphics, path.exists());
        assert_eq!(p, Plan { mark: true, fell_back: false });
        apply(&path, &p, &mut s);
        assert!(path.exists(), "the mark before the window");
        assert_eq!(s.graphics, Graphics::Vulkan);
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn no_mark_with_directx12() {
        let path = temp("dx12");
        let mut s = Settings::default();
        let p = plan(true, s.graphics, false);
        assert_eq!(p, Plan::default());
        apply(&path, &p, &mut s);
        assert!(!path.exists());
        assert_eq!(s.graphics, Graphics::Directx12);
    }

    #[test]
    fn mark_found_goes_back_to_directx12() {
        let path = temp("found");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        apply(&path, &plan(true, s.graphics, false), &mut s);
        // The start with Vulkan ended before its first frame: the mark is still there.
        let p = plan(true, s.graphics, path.exists());
        assert_eq!(p, Plan { mark: false, fell_back: true });
        apply(&path, &p, &mut s);
        assert_eq!(s.graphics, Graphics::Directx12, "DirectX 12 back in the Options");
        assert!(!path.exists(), "the old mark taken away");
        // The start after: DirectX 12, nothing to say.
        assert_eq!(plan(true, s.graphics, path.exists()), Plan::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    /// DirectX 12 chosen by hand after a start with Vulkan that never drew: the mark goes away
    /// and nothing is said.
    #[test]
    fn mark_found_with_directx12_chosen() {
        let path = temp("chosen");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "x").unwrap();
        let mut s = Settings::default();
        let p = plan(true, s.graphics, path.exists());
        assert_eq!(p, Plan::default());
        apply(&path, &p, &mut s);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn mark_taken_away_after_first_frame() {
        let path = temp("frame");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        apply(&path, &plan(true, s.graphics, false), &mut s);
        assert!(!take_after_first(&path, 0), "not before a frame is drawn");
        assert!(path.exists());
        assert!(take_after_first(&path, 1));
        assert!(!path.exists());
        // The next start with Vulkan: no fall back.
        assert_eq!(plan(true, s.graphics, path.exists()), Plan { mark: true, fell_back: false });
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
