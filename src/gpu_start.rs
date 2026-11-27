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
//! Where no graphics card draws with DirectX 12 and Vulkan never drew, the driver does not work:
//! that is remembered (`driver-broken`), and every start draws with DirectX 12 alone, which is
//! then Windows' own software renderer (WARP, "Microsoft Basic Render Driver", in every Windows
//! 10 and 11), and says to update the driver, in place of the tip of the day. No start goes back
//! to Vulkan by itself, so starts never alternate between a crash and a window. Choosing Vulkan in
//! the Options tries the card again; a card that draws with DirectX 12 (after an update) ends it
//! by itself. A window that cannot be made at all is told with a message of Windows (`no_window`).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::{egui, wgpu};

use crate::settings::{Graphics, Settings};

/// The mark's file, beside `settings.json` (the test copy has its own folder).
const MARK: &str = "vulkan-start";
/// The file that says the driver does not work, beside the mark.
const BROKEN: &str = "driver-broken";

/// This start wrote the mark: taken away after the first frame (`frame_drawn`).
static MARKED: AtomicBool = AtomicBool::new(false);
/// This start went back to DirectX 12 after a start with Vulkan that never drew.
static FELL_BACK: AtomicBool = AtomicBool::new(false);
/// The driver does not work: this start draws without the graphics card.
static BROKEN_NOW: AtomicBool = AtomicBool::new(false);

/// What this start does with the graphics card.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    /// What wgpu is asked for; `None`: eframe's own choice (`WGPU_BACKEND`, or every kind).
    pub(crate) backends: Option<wgpu::Backends>,
    /// The mark is written before the window: Vulkan may be loaded.
    pub(crate) mark: bool,
    /// The last start with Vulkan ended before its first frame: back to DirectX 12.
    pub(crate) fell_back: bool,
    /// No card draws with DirectX 12 and Vulkan never drew: DirectX 12 alone (Windows' software
    /// renderer) at every start, until Vulkan is chosen again or a card draws with DirectX 12.
    pub(crate) broken: bool,
}

/// The plan for a start: on Windows only, where the choice exists. `env`: what `WGPU_BACKEND`
/// asks for; `marked_before`: the mark of an earlier start is still there; `broken_before`: the
/// driver was found not to work; `real_dx12`: a graphics card (not the software one) draws with
/// DirectX 12, asked only when it matters, at most once.
/// A mark found means DirectX 12 alone, also when the Options already say DirectX 12 and Vulkan
/// came in because no card drew with it: otherwise that computer would stop at every start. With
/// no card that draws with DirectX 12 that is the software renderer, and it stays (`broken`)
/// until Vulkan is chosen again (Vulkan in the Options with no mark: chosen after the fall back).
/// `WGPU_BACKEND` leaves the state as it is.
pub(crate) fn plan(windows: bool, graphics: Graphics, env: Option<wgpu::Backends>, marked_before: bool, broken_before: bool, real_dx12: impl FnOnce() -> bool) -> Plan {
    if !windows {
        return Plan::default();
    }
    let mut ask = Some(real_dx12);
    let mut known = None;
    let mut real = || *known.get_or_insert_with(|| ask.take().is_some_and(|f| f()));
    let fell_back = marked_before && graphics == Graphics::Vulkan;
    let retry = graphics == Graphics::Vulkan && !marked_before;
    let broken = if env.is_some() { broken_before } else { !retry && (marked_before || broken_before) && !real() };
    let dx12_only = env.is_none() && (marked_before || broken || (graphics == Graphics::Directx12 && real()));
    let backends = dx12_only.then_some(wgpu::Backends::DX12);
    let asked = backends.or(env).unwrap_or(wgpu::Backends::all());
    Plan { backends, mark: asked.contains(wgpu::Backends::VULKAN), fell_back, broken }
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

/// Carries out the plan on the files in the configuration folder `dir` and on the settings
/// (saved by the caller): an old mark is always taken away, a new one written when Vulkan may be
/// used; the file of the broken driver there exactly when the plan says so.
pub(crate) fn apply(dir: &Path, plan: &Plan, settings: &mut Settings) {
    if plan.fell_back {
        settings.graphics = Graphics::Directx12;
    }
    if plan.mark || plan.broken {
        let _ = std::fs::create_dir_all(dir);
    }
    if plan.mark {
        let _ = std::fs::write(dir.join(MARK), format!("EyeCanSee {} starting with Vulkan\n", crate::version()));
    } else {
        let _ = std::fs::remove_file(dir.join(MARK));
    }
    if plan.broken {
        let _ = std::fs::write(dir.join(BROKEN), "No graphics card drew: DirectX 12 alone (software) until Vulkan is chosen again\n");
    } else {
        let _ = std::fs::remove_file(dir.join(BROKEN));
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
    let dir = path.parent()?;
    let plan = plan(true, settings.graphics, wgpu::Backends::from_env(), path.exists(), dir.join(BROKEN).exists(), real_dx12);
    apply(dir, &plan, settings);
    if plan.fell_back {
        settings.save();
    }
    log!("graphics: {:?}, asked {:?}, mark {}, back to DirectX 12 {}, driver broken {}", settings.graphics, plan.backends, plan.mark, plan.fell_back, plan.broken);
    MARKED.store(plan.mark, Ordering::Relaxed);
    FELL_BACK.store(plan.fell_back, Ordering::Relaxed);
    BROKEN_NOW.store(plan.broken, Ordering::Relaxed);
    plan.backends
}

/// What the user is told at this start in place of the tip of the day, as Fluent ids of the title
/// and the text (`App::new`): the broken driver, at every start while it lasts, before the fall
/// back to DirectX 12, which a broken driver also is.
pub(crate) fn notice() -> Option<(&'static str, &'static str)> {
    notice_for(BROKEN_NOW.load(Ordering::Relaxed), false, FELL_BACK.load(Ordering::Relaxed))
}

fn notice_for(broken: bool, _broken_before: bool, fell_back: bool) -> Option<(&'static str, &'static str)> {
    if broken {
        Some(("graphics-broken-title", "graphics-broken-text"))
    } else if fell_back {
        Some(("vulkan-failed-title", "vulkan-failed-text"))
    } else {
        None
    }
}

/// The window could not be made at all (`eframe::run_native` gave an error, which would otherwise
/// close the program without a word): on Windows a message of the system says to update the
/// graphics card's driver, with the error under it. `language`: the one in the Options.
pub(crate) fn no_window(language: Option<&str>, error: &eframe::Error) {
    log!("graphics: no window: {error}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let i18n = crate::i18n::I18n::detect(language);
        let text = format!("{}\n\n{error}", i18n.t("graphics-no-window"));
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (text, title) = (wide(&text), wide("EyeCanSee"));
        // SAFETY: two strings ending with a zero, alive for the call; no owner window.
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
    }
    // Linux: the desktop's own message window, GNOME's (zenity) or KDE's (kdialog), whichever is
    // there; with neither, only the terminal. Before, the program closed without a word (#319).
    #[cfg(not(windows))]
    {
        let i18n = crate::i18n::I18n::detect(language);
        let text = format!("{}\n\n{error}", i18n.t("graphics-no-window"));
        eprintln!("EyeCanSee: {text}");
        let shown = |cmd: &str, args: &[&str]| std::process::Command::new(cmd).args(args).status().is_ok_and(|s| s.success());
        // ponytail: two common programs, no D-Bus portal; a desktop without either gets the terminal.
        let _ = shown("zenity", &["--error", "--title=EyeCanSee", "--no-markup", &format!("--text={text}")])
            || shown("kdialog", &["--title", "EyeCanSee", "--error", &text]);
    }
}

/// At every frame: the mark goes away once the first frame has been drawn and shown (`frames`
/// counts the frames completed: eframe paints each one right after `App::ui`). Until then a
/// repaint is asked, so it does not wait for the mouse.
pub(crate) fn frame_drawn(ctx: &egui::Context) {
    if !MARKED.load(Ordering::Relaxed) {
        return;
    }
    let Some(path) = crate::settings::config_path(MARK) else { return };
    if take_after_first(&path, ctx.cumulative_frame_nr(), std::time::Duration::ZERO) {
        MARKED.store(false, Ordering::Relaxed);
        log!("graphics: first frame drawn, mark taken away");
    } else {
        ctx.request_repaint();
    }
}

/// The mark at `path` taken away once `frames` (completed) is at least one; true when done.
fn take_after_first(path: &Path, frames: u64, _alive: std::time::Duration) -> bool {
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

    /// An empty configuration folder of its own.
    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-gpu-start-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("eyecansee")
    }

    fn clean(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// The plan for the files in `dir` as they are.
    fn plan_in(dir: &Path, graphics: Graphics, real_dx12: impl FnOnce() -> bool) -> Plan {
        plan(true, graphics, None, dir.join(MARK).exists(), dir.join(BROKEN).exists(), real_dx12)
    }

    /// The question about the cards, when it must not be asked.
    fn unasked() -> bool {
        panic!("the DirectX 12 cards asked for nothing")
    }

    const fn p(backends: Option<wgpu::Backends>, mark: bool, fell_back: bool, broken: bool) -> Plan {
        Plan { backends, mark, fell_back, broken }
    }

    #[test]
    fn only_on_windows() {
        for g in [Graphics::Directx12, Graphics::Vulkan] {
            for m in [false, true] {
                assert_eq!(plan(false, g, None, m, m, unasked), Plan::default());
            }
        }
    }

    #[test]
    fn mark_written_with_vulkan() {
        let dir = temp("written");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        let plan = plan_in(&dir, s.graphics, unasked);
        assert_eq!(plan, p(None, true, false, false), "every kind, Vulkan first");
        apply(&dir, &plan, &mut s);
        assert!(dir.join(MARK).exists(), "the mark before the window");
        assert_eq!(s.graphics, Graphics::Vulkan);
        clean(&dir);
    }

    /// DirectX 12 with a card that draws with it: DirectX 12 alone, Vulkan's driver not loaded.
    #[test]
    fn directx12_alone_and_no_mark() {
        let dir = temp("dx12");
        let mut s = Settings::default();
        let plan = plan_in(&dir, s.graphics, || true);
        assert_eq!(plan, p(DX12, false, false, false));
        apply(&dir, &plan, &mut s);
        assert!(!dir.join(MARK).exists());
        assert!(!dir.join(BROKEN).exists());
        assert_eq!(s.graphics, Graphics::Directx12);
    }

    /// No card draws with DirectX 12 (the software one alone, or none): every kind, as up to
    /// 0.77.0, and the mark, since Vulkan may be used.
    #[test]
    fn directx12_without_a_card_asks_every_kind() {
        assert_eq!(plan(true, Graphics::Directx12, None, false, false, || false), p(None, true, false, false));
    }

    /// Vulkan chosen, it never drew, and a card draws with DirectX 12: back to it, said once.
    #[test]
    fn mark_found_goes_back_to_directx12() {
        let dir = temp("found");
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        apply(&dir, &plan_in(&dir, s.graphics, unasked), &mut s);
        // The start with Vulkan ended before its first frame: the mark is still there.
        let plan = plan_in(&dir, s.graphics, || true);
        assert_eq!(plan, p(DX12, false, true, false));
        apply(&dir, &plan, &mut s);
        assert_eq!(s.graphics, Graphics::Directx12, "DirectX 12 back in the Options");
        assert!(!dir.join(MARK).exists(), "the old mark taken away");
        assert!(!dir.join(BROKEN).exists());
        // The start after: DirectX 12, nothing to say.
        assert_eq!(plan_in(&dir, s.graphics, || true), p(DX12, false, false, false));
        clean(&dir);
    }

    /// DirectX 12 in the Options and the mark there, with a card that draws with DirectX 12
    /// (chosen by hand after a start with Vulkan that never drew): DirectX 12 alone, nothing said.
    #[test]
    fn mark_found_with_directx12_chosen() {
        let dir = temp("chosen");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MARK), "x").unwrap();
        let mut s = Settings::default();
        let plan = plan_in(&dir, s.graphics, || true);
        assert_eq!(plan, p(DX12, false, false, false));
        apply(&dir, &plan, &mut s);
        assert!(!dir.join(MARK).exists());
        assert!(!dir.join(BROKEN).exists());
        clean(&dir);
    }

    /// No card draws with DirectX 12 and Vulkan never drew, whatever the Options said: the driver
    /// does not work. Every start after draws with DirectX 12 alone (the software renderer) and
    /// says so, and none goes back to Vulkan by itself: no crash, window, crash, window.
    #[test]
    fn no_card_that_works_stays_on_the_software_renderer() {
        for chosen in [Graphics::Vulkan, Graphics::Directx12] {
            let dir = temp("broken");
            let mut s = Settings { graphics: chosen, ..Settings::default() };
            let first = plan_in(&dir, s.graphics, || false);
            assert!(first.mark, "Vulkan may be used: {first:?}");
            apply(&dir, &first, &mut s);
            // Vulkan closed the program: the mark is still there.
            let plan = plan_in(&dir, s.graphics, || false);
            assert_eq!(plan, p(DX12, false, chosen == Graphics::Vulkan, true));
            apply(&dir, &plan, &mut s);
            assert_eq!(s.graphics, Graphics::Directx12);
            assert!(!dir.join(MARK).exists());
            assert!(dir.join(BROKEN).exists(), "remembered");
            assert_eq!(notice_for(plan.broken, false, plan.fell_back), Some(("graphics-broken-title", "graphics-broken-text")));
            // The starts after: the same, with no mark, so Vulkan is never loaded again.
            for _ in 0..3 {
                let again = plan_in(&dir, s.graphics, || false);
                assert_eq!(again, p(DX12, false, false, true));
                apply(&dir, &again, &mut s);
                assert_eq!(notice_for(again.broken, true, again.fell_back), Some(("graphics-broken-title", "graphics-broken-text")), "said at every start");
            }
            clean(&dir);
        }
    }

    /// Vulkan chosen in the Options while the driver does not work: the card tried again.
    #[test]
    fn vulkan_chosen_tries_the_card_again() {
        let dir = temp("retry");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(BROKEN), "x").unwrap();
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        let plan = plan_in(&dir, s.graphics, unasked);
        assert_eq!(plan, p(None, true, false, false));
        apply(&dir, &plan, &mut s);
        assert!(!dir.join(BROKEN).exists());
        assert!(dir.join(MARK).exists());
        // It closed the program again: back to the software renderer, not to an alternation.
        assert_eq!(plan_in(&dir, s.graphics, || false), p(DX12, false, true, true));
        clean(&dir);
    }

    /// A card that draws with DirectX 12 appears (after a driver update): the state ends by itself.
    #[test]
    fn a_card_for_directx12_ends_it() {
        let dir = temp("updated");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(BROKEN), "x").unwrap();
        let mut s = Settings::default();
        let plan = plan_in(&dir, s.graphics, || true);
        assert_eq!(plan, p(DX12, false, false, false));
        apply(&dir, &plan, &mut s);
        assert!(!dir.join(BROKEN).exists());
        assert_eq!(notice_for(plan.broken, true, plan.fell_back), None, "the tip of the day again");
        clean(&dir);
    }

    /// Audit A.2: the broken driver is said at the start that finds it; the starts after keep the
    /// first-start window and the tip of the day. Its text holds at any start, whoever ended the
    /// one with Vulkan.
    #[test]
    fn broken_driver_said_at_the_start_that_finds_it() {
        assert_eq!(notice_for(true, false, false), Some(("graphics-broken-title", "graphics-broken-text")));
        assert_eq!(notice_for(true, true, false), None, "found at an earlier start");
        for (lang, ftl) in [("en", include_str!("../locales/en.ftl")), ("it", include_str!("../locales/it.ftl"))] {
            let text = ftl.lines().find_map(|l| l.strip_prefix("graphics-broken-text = ")).unwrap();
            assert!(!text.contains("At the last start") && !text.contains("All'ultimo avvio"), "{lang}: {text}");
        }
    }

    /// Audit A.2: a window hidden from the start (minimised) draws no frame: the mark goes away
    /// after a while of life all the same, or the next start would take the driver for broken.
    #[test]
    fn mark_taken_away_after_a_while_hidden() {
        let dir = temp("hidden");
        let path = dir.join(MARK);
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        apply(&dir, &plan_in(&dir, s.graphics, unasked), &mut s);
        assert!(!take_after_first(&path, 0, std::time::Duration::from_secs(1)), "not at once");
        assert!(take_after_first(&path, 0, std::time::Duration::from_secs(30)), "after a while");
        assert!(!path.exists());
        clean(&dir);
    }

    #[test]
    fn notices() {
        assert_eq!(notice_for(false, false, false), None);
        assert_eq!(notice_for(false, false, true), Some(("vulkan-failed-title", "vulkan-failed-text")));
        assert_eq!(notice_for(true, false, true), Some(("graphics-broken-title", "graphics-broken-text")));
    }

    /// The cards are asked about once at most, also when two answers need them.
    #[test]
    fn cards_asked_once() {
        let mut asked = 0;
        let plan = plan(true, Graphics::Directx12, None, false, true, || {
            asked += 1;
            true
        });
        assert_eq!(plan, p(DX12, false, false, false));
        assert_eq!(asked, 1);
    }

    /// `WGPU_BACKEND` wins: eframe asks for what it says, the mark follows it, and the state of
    /// the driver stays as it is.
    #[test]
    fn wgpu_backend_wins() {
        assert_eq!(plan(true, Graphics::Vulkan, DX12, false, false, unasked), Plan::default());
        assert_eq!(plan(true, Graphics::Directx12, Some(wgpu::Backends::VULKAN), false, false, unasked), p(None, true, false, false));
        assert_eq!(plan(true, Graphics::Vulkan, DX12, true, false, unasked), p(None, false, true, false));
        assert_eq!(plan(true, Graphics::Directx12, DX12, false, true, unasked), p(None, false, false, true));
    }

    #[test]
    fn mark_taken_away_after_first_frame() {
        let dir = temp("frame");
        let path = dir.join(MARK);
        let mut s = Settings { graphics: Graphics::Vulkan, ..Settings::default() };
        apply(&dir, &plan_in(&dir, s.graphics, unasked), &mut s);
        assert!(!take_after_first(&path, 0, std::time::Duration::ZERO), "not before a frame is drawn");
        assert!(path.exists());
        assert!(take_after_first(&path, 1, std::time::Duration::ZERO));
        assert!(!path.exists());
        // The next start with Vulkan: no fall back.
        assert_eq!(plan_in(&dir, s.graphics, unasked), p(None, true, false, false));
        clean(&dir);
    }
}
