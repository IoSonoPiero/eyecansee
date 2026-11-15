//! Windows: the window at start inside the free area of its screen, centred (#109).
//!
//! winit creates the window where Windows puts new windows (each start shifted from the last),
//! and egui's `centered` centres it on the whole screen, taskbar included. Here the window, still
//! hidden, is moved within the work area of the monitor Windows chose for it, and made smaller
//! if it does not fit. Everything is in physical pixels of that one monitor: the window does not
//! change monitor, so its scale (DPI) does not change and nothing is converted.

/// A rectangle in physical pixels, as Win32 gives it (right and bottom excluded).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Area {
    fn width(&self) -> i32 {
        self.right - self.left
    }

    fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Where the whole window (title bar and borders included) of `width` × `height` goes in `work`:
/// no larger than the work area, centred in it.
pub fn fit(work: Area, width: i32, height: i32) -> Area {
    let width = width.clamp(1, work.width().max(1));
    let height = height.clamp(1, work.height().max(1));
    let left = work.left + (work.width() - width) / 2;
    let top = work.top + (work.height() - height) / 2;
    Area { left, top, right: left + width, bottom: top + height }
}

/// Moves the window that eframe has just made (hidden until its first frame) into the work
/// area of its monitor. A maximized window is left alone: it already fills the work area, and
/// changing its restored place would mean `SetWindowPlacement`, which would show it early.
#[cfg(windows)]
pub fn place(cc: &eframe::CreationContext) {
    use eframe::egui_wgpu::wgpu::rwh::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsZoomed, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};

    let Ok(RawWindowHandle::Win32(handle)) = cc.window_handle().map(|h| h.as_raw()) else {
        return;
    };
    let hwnd = handle.hwnd.get() as HWND;
    let area = |r: RECT| Area { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    // SAFETY: `hwnd` is the live window eframe has just created, on this thread; the structures
    // are plain data, with `cbSize` set as `GetMonitorInfoW` requires.
    unsafe {
        if IsZoomed(hwnd) != 0 {
            return;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        let mut rect: RECT = std::mem::zeroed();
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if GetMonitorInfoW(monitor, &mut info) == 0 || GetWindowRect(hwnd, &mut rect) == 0 {
            return;
        }
        let (work, now) = (area(info.rcWork), area(rect));
        let to = fit(work, now.width(), now.height());
        log!("window at start: {now:?} in work area {work:?} -> {to:?}");
        SetWindowPos(hwnd, std::ptr::null_mut(), to.left, to.top, to.width(), to.height(), SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, fit};

    const fn area(left: i32, top: i32, right: i32, bottom: i32) -> Area {
        Area { left, top, right, bottom }
    }

    #[test]
    fn centred_when_it_fits() {
        // 1920 × 1080 with a 48 px taskbar at the bottom; 1200 × 800 at 100 % plus the frame.
        let work = area(0, 0, 1920, 1032);
        assert_eq!(fit(work, 1216, 839), area(352, 96, 1568, 935));
    }

    #[test]
    fn smaller_when_it_does_not_fit() {
        // The case of #109: 2048 × 1020 with the taskbar, at 150 % 1200 × 800 is 1800 × 1200.
        let work = area(0, 0, 2048, 972);
        assert_eq!(fit(work, 1822, 1245), area(113, 0, 1935, 972));
        // Too wide as well: the whole work area.
        let work = area(0, 0, 1366, 720);
        assert_eq!(fit(work, 1822, 1245), work);
    }

    #[test]
    fn taskbar_at_the_top_or_on_a_side() {
        let top = area(0, 48, 1920, 1080);
        assert_eq!(fit(top, 1216, 839), area(352, 144, 1568, 983));
        let left = area(62, 0, 1920, 1080);
        assert_eq!(fit(left, 1216, 839), area(383, 120, 1599, 959));
    }

    #[test]
    fn on_a_second_monitor() {
        // Left of the main one (negative coordinates) and above it.
        assert_eq!(fit(area(-2560, 0, 0, 1392), 1216, 839), area(-1888, 276, -672, 1115));
        assert_eq!(fit(area(0, -1440, 2560, -40), 1216, 839), area(672, -1160, 1888, -321));
    }

    #[test]
    fn never_empty() {
        assert_eq!(fit(area(10, 10, 10, 10), 1216, 839), area(10, 10, 11, 11));
        assert_eq!(fit(area(0, 0, 800, 600), 0, -5), area(399, 299, 400, 300));
    }
}
