//! Windows: the title bar painted at once in the theme sent to the window. winit sets it with the
//! undocumented `SetWindowCompositionAttribute`, which changes the colour of the title and the
//! buttons but leaves the bar as it was until the window is activated again: with Windows light
//! and EyeCanSee dark the bar stayed white with white title and buttons, invisible, from the start
//! until the window lost and got back the focus. Here, the frame after winit's, the documented
//! `DWMWA_USE_IMMERSIVE_DARK_MODE` is set too and the frame is redrawn (`SWP_FRAMECHANGED`).

use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};

/// The main window, known once eframe has made it.
static WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Keeps the main window's handle, as eframe has just made it.
pub fn remember(cc: &eframe::CreationContext) {
    use eframe::egui_wgpu::wgpu::rwh::{HasWindowHandle, RawWindowHandle};
    if let Ok(RawWindowHandle::Win32(handle)) = cc.window_handle().map(|h| h.as_raw()) {
        WINDOW.store(handle.hwnd.get(), Ordering::SeqCst);
    }
}

/// The title bar dark or light now.
pub fn paint(dark: bool) {
    let hwnd = WINDOW.load(Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
        return;
    }
    let value: i32 = dark.into();
    // SAFETY: `hwnd` is the main window, alive while the app runs (this is called from its frame);
    // the attribute takes a 4-byte BOOL, given with its size.
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&raw const value).cast(), size_of::<i32>() as u32);
        SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
