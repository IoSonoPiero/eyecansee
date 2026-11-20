//! A start with Vulkan that never drew (Windows): a broken Vulkan driver can bring the program
//! down inside its own code before the first frame, where no code of ours can catch it (Intel's
//! `igvk64.dll`, Store user report of 20/11). Before the window is made with Vulkan a mark is
//! written in the configuration folder (`vulkan-start`), and taken away once the first frame has
//! been drawn. A mark found at the next start means the last one with Vulkan ended before that:
//! this one goes back to DirectX 12, puts it back in the Options and says why (`App::new`).
//! Closing the program while it starts, or a second start while the first one is still slow to
//! draw, does the same: harmless, DirectX 12 is the default.
//! With DirectX 12 wgpu is asked for DirectX 12 alone, so a broken Vulkan driver is not even
//! loaded; every kind (Vulkan first, as `render_gpu::adapter_selector` sorts them) only where no
//! graphics card draws with DirectX 12. `WGPU_BACKEND` (`dx12`, `vulkan`) still wins over both.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::{egui, wgpu};

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
    /// What wgpu is asked for; `None`: eframe's own choice (`WGPU_BACKEND`, or every kind).
    pub(crate) backends: Option<wgpu::Backends>,
    /// The mark is written before the window: Vulkan may be loaded.
    pub(crate) mark: bool,
    /// The last start with Vulkan ended before its first frame: back to DirectX 12.
    pub(crate) fell_back: bool,
}

/// The plan for a start: on Windows only, where the choice exists. `env`: what `WGPU_BACKEND`
/// asks for; `marked_before`: the mark of an earlier start is still there; `real_dx12`: a
/// graphics card (not the software one) draws with DirectX 12, asked only when it matters.
/// A mark found means DirectX 12 alone, also when the Options already say DirectX 12 and Vulkan
/// came in because no card drew with it: otherwise that computer would stop at every start.
pub(crate) fn plan(windows: bool, graphics: Graphics, env: Option<wgpu::Backends>, marked_before: bool, real_dx12: impl FnOnce() -> bool) -> Plan {
    if !windows {
        return Plan::default();
    }
    let fell_back = marked_before && graphics == Graphics::Vulkan;
    let dx12_only = env.is_none() && (marked_before || (graphics == Graphics::Directx12 && real_dx12()));
    let backends = dx12_only.then_some(wgpu::Backends::DX12);
    let asked = backends.or(env).unwrap_or(wgpu::Backends::all());
    Plan { backends, mark: asked.contains(wgpu::Backends::VULKAN), fell_back }
}

/// A graphics card draws with DirectX 12: the software one (Microsoft Basic Render Driver, which
/// wgpu lists as a CPU) does not count. Asked of an instance with DirectX 12 alone, which does
/// not load Vulkan's driver.
fn real_dx12() -> bool {
    use std::future::Future;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::DX12, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let mut list = std::pin::pin!(instance.enumerate_adapters(wgpu::Backends::DX12));
    // Outside the web the list is made at once: ready at the first poll (wgpu 30).
    let std::task::Poll::Ready(adapters) = list.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) else { return false };
    let names: Vec<String> = adapters.iter().map(|a| format!("{} ({:?})", a.get_info().name, a.get_info().device_type)).collect();
    log!("graphics: DirectX 12 cards {names:?}");
    adapters.iter().any(|a| a.get_info().device_type != wgpu::DeviceType::Cpu)
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
/// and saved when this start goes back to DirectX 12. What to ask wgpu for (`None`: eframe's own
/// choice). Nothing outside Windows.
pub(crate) fn before_window(settings: &mut Settings) -> Option<wgpu::Backends> {
    if !cfg!(windows) {
        return None;
    }
    let path = crate::settings::config_path(MARK)?;
    let plan = plan(true, settings.graphics, wgpu::Backends::from_env(), path.exists(), real_dx12);
    apply(&path, &plan, settings);
    if plan.fell_back {
        settings.save();
    }
    log!("graphics: {:?}, asked {:?}, mark {}, back to DirectX 12 {}", settings.graphics, plan.backends, plan.mark, plan.fell_back);
    MARKED.store(plan.mark, Ordering::Relaxed);
    FELL_BACK.store(plan.fell_back, Ordering::Relaxed);
    plan.backends
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

    const DX12: Option<wgpu::Backends> = Some(wgpu::Backends::DX12);

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-gpu-start-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("eyecansee").join(MARK)
    }

    fn clean(path: &Path) {
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    /// The question about the cards, when it must not be asked.
    fn unasked() -> bool {
        panic!("the DirectX 12 cards asked for nothing")
    }

    #[test]
    fn only_on_windows() {
        for g in [Graphics::Directx12, Graphics::Vulkan] {
            for m in [false, true] {
                assert_eq!(plan(false, g, None, m, unasked), Plan::default());
            }
        }
    }

    #[test]
    fn mark_written_with_vulkan() {
        let path = temp("written");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        let p = plan(true, s.graphics, None, path.exists(), unasked);
        assert_eq!(p, Plan { backends: None, mark: true, fell_back: false }, "every kind, Vulkan first");
        apply(&path, &p, &mut s);
        assert!(path.exists(), "the mark before the window");
        assert_eq!(s.graphics, Graphics::Vulkan);
        clean(&path);
    }

    /// DirectX 12 with a card that draws with it: DirectX 12 alone, Vulkan's driver not loaded.
    #[test]
    fn directx12_alone_and_no_mark() {
        let path = temp("dx12");
        let mut s = Settings::default();
        let p = plan(true, s.graphics, None, false, || true);
        assert_eq!(p, Plan { backends: DX12, mark: false, fell_back: false });
        apply(&path, &p, &mut s);
        assert!(!path.exists());
        assert_eq!(s.graphics, Graphics::Directx12);
    }

    /// No card draws with DirectX 12 (the software one alone, or none): every kind, as up to
    /// 0.77.0, and the mark, since Vulkan may be used.
    #[test]
    fn directx12_without_a_card_asks_every_kind() {
        assert_eq!(plan(true, Graphics::Directx12, None, false, || false), Plan { backends: None, mark: true, fell_back: false });
    }

    #[test]
    fn mark_found_goes_back_to_directx12() {
        let path = temp("found");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        apply(&path, &plan(true, s.graphics, None, false, unasked), &mut s);
        // The start with Vulkan ended before its first frame: the mark is still there.
        let p = plan(true, s.graphics, None, path.exists(), unasked);
        assert_eq!(p, Plan { backends: DX12, mark: false, fell_back: true });
        apply(&path, &p, &mut s);
        assert_eq!(s.graphics, Graphics::Directx12, "DirectX 12 back in the Options");
        assert!(!path.exists(), "the old mark taken away");
        // The start after: DirectX 12, nothing to say.
        assert_eq!(plan(true, s.graphics, None, path.exists(), || true), Plan { backends: DX12, mark: false, fell_back: false });
        clean(&path);
    }

    /// DirectX 12 in the Options and the mark there (chosen by hand after a start with Vulkan
    /// that never drew, or Vulkan used because no card drew with DirectX 12): DirectX 12 alone,
    /// whatever the cards, and nothing said.
    #[test]
    fn mark_found_with_directx12_chosen() {
        let path = temp("chosen");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "x").unwrap();
        let mut s = Settings::default();
        let p = plan(true, s.graphics, None, path.exists(), unasked);
        assert_eq!(p, Plan { backends: DX12, mark: false, fell_back: false });
        apply(&path, &p, &mut s);
        assert!(!path.exists());
        clean(&path);
    }

    /// `WGPU_BACKEND` wins: eframe asks for what it says, and the mark follows it.
    #[test]
    fn wgpu_backend_wins() {
        assert_eq!(plan(true, Graphics::Vulkan, DX12, false, unasked), Plan::default());
        assert_eq!(plan(true, Graphics::Directx12, Some(wgpu::Backends::VULKAN), false, unasked), Plan { backends: None, mark: true, fell_back: false });
        assert_eq!(plan(true, Graphics::Vulkan, DX12, true, unasked), Plan { backends: None, mark: false, fell_back: true });
    }

    #[test]
    fn mark_taken_away_after_first_frame() {
        let path = temp("frame");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        apply(&path, &plan(true, s.graphics, None, false, unasked), &mut s);
        assert!(!take_after_first(&path, 0), "not before a frame is drawn");
        assert!(path.exists());
        assert!(take_after_first(&path, 1));
        assert!(!path.exists());
        // The next start with Vulkan: no fall back.
        assert_eq!(plan(true, s.graphics, None, path.exists(), unasked), Plan { backends: None, mark: true, fell_back: false });
        clean(&path);
    }
}
