//! Windows: the title bar painted at once in the theme sent to the window. winit sets it with the
//! undocumented `SetWindowCompositionAttribute`, which changes the colour of the title and the
//! buttons but leaves the bar as it was until the window is activated again: with Windows light
//! and EyeCanSee dark the bar stayed white with white title and buttons, invisible, from the start
//! until the window lost and got back the focus. Here the documented
//! `DWMWA_USE_IMMERSIVE_DARK_MODE` is set while the window is still hidden, so it appears with the
//! right bar; on a later change (Options, the system's theme) it is set the frame after winit's
//! and the bar is redrawn as Windows does when the window is activated (`WM_NCACTIVATE` twice:
//! `SWP_FRAMECHANGED` alone did not repaint it).

use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SendMessageW, WM_NCACTIVATE};

/// The main window, known once eframe has made it.
static WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Keeps the main window's handle, as eframe has just made it (still hidden), and gives it its
/// bar: `dark` from the Options, or the system's (`None` when not known yet: winit's then).
pub fn remember(cc: &eframe::CreationContext, dark: Option<bool>) {
    use eframe::egui_wgpu::wgpu::rwh::{HasWindowHandle, RawWindowHandle};
    if let Ok(RawWindowHandle::Win32(handle)) = cc.window_handle().map(|h| h.as_raw()) {
        WINDOW.store(handle.hwnd.get(), Ordering::SeqCst);
        if let Some(dark) = dark {
            set(handle.hwnd.get() as HWND, dark);
        }
    }
}

fn set(hwnd: HWND, dark: bool) {
    let value: i32 = dark.into();
    // SAFETY: `hwnd` is the main window, alive while the app runs; the attribute takes a 4-byte
    // BOOL, given with its size.
    unsafe { DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&raw const value).cast(), size_of::<i32>() as u32) };
}

/// The title bar dark or light now, on a window already shown.
pub fn paint(dark: bool) {
    let hwnd = WINDOW.load(Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
        return;
    }
    set(hwnd, dark);
    // SAFETY: as above; WM_NCACTIVATE only redraws the frame, active or not as it is now.
    unsafe {
        let active = usize::from(GetForegroundWindow() == hwnd);
        SendMessageW(hwnd, WM_NCACTIVATE, 1 - active, 0);
        SendMessageW(hwnd, WM_NCACTIVATE, active, 0);
    }
}
