//! The apps' light or dark mode on Windows, read from the registry: `AppsUseLightTheme` under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`.
//! winit 0.30.13 reads it at start through uxtheme's `ShouldAppsUseDarkMode`, right, but on a
//! change (`WM_SETTINGCHANGE`) that function still answers with the value before: winit never
//! asks uxtheme to refresh it (`RefreshImmersiveColorPolicyState`), so the window followed every
//! change one step late (dark stayed light, then light turned dark). Here the value is read on a
//! thread of its own as the program starts, then again at each change of the key
//! (`RegNotifyChangeKeyValue`): the Settings app and the Personalize page write it there.
//! No key or no value: light, Windows' default.
//! The title bar lagged the same way: on that message winit sets it from the same stale reading,
//! even when a theme was set on the window (its `set_theme` does not keep it as the preferred
//! one). So at each change uxtheme is refreshed first (the key is written before the message is
//! sent), and a moment later the window's own theme is sent again (`super::resend_window`), over
//! the system's that winit put back when Light or Dark is chosen in the Options.

use eframe::egui::Theme;
use std::time::Duration;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, RRF_RT_REG_DWORD, RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue,
    RegOpenKeyExW,
};

const PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const VALUE: &str = "AppsUseLightTheme";

/// The theme for `AppsUseLightTheme` (`None`: no such value): 0 is dark, anything else light.
pub fn from_registry(value: Option<u32>) -> Theme {
    match value {
        Some(0) => Theme::Dark,
        _ => Theme::Light,
    }
}

/// Starts reading and following the preference, in the background.
pub(super) fn start() {
    let spawned = std::thread::Builder::new().name("system-theme".into()).spawn(follow);
    if spawned.is_err() {
        store(None);
    }
}

fn store(value: Option<u32>) {
    let theme = from_registry(value);
    log!("system theme: {VALUE} {value:?}, {theme:?}");
    super::store(theme);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The first value, then every change of the key, as long as it can be watched.
fn follow() {
    let path = wide(PATH);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `path` ends with a NUL; `key` is written only on success.
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) } != ERROR_SUCCESS {
        store(None);
        return;
    }
    store(read(key));
    loop {
        // Waits here, synchronously, until something under the key is written.
        // SAFETY: `key` is open with KEY_NOTIFY (part of KEY_READ); no event, so no handle.
        let watched = unsafe { RegNotifyChangeKeyValue(key, 0, REG_NOTIFY_CHANGE_LAST_SET, std::ptr::null_mut(), 0) };
        if watched != ERROR_SUCCESS {
            // The value read stays: an error here must not turn it into light.
            log!("system theme: changes not followed: {watched}");
            break;
        }
        refresh_uxtheme();
        store(read(key));
        // After the window has had the message, which sets the title bar to the system's.
        std::thread::sleep(Duration::from_millis(500));
        super::resend_window();
    }
    // SAFETY: `key` was opened above and is not used after.
    unsafe { RegCloseKey(key) };
}

/// Has uxtheme read the apps' mode again (`RefreshImmersiveColorPolicyState`, uxtheme ordinal
/// 104, undocumented like `ShouldAppsUseDarkMode`, ordinal 132, that winit reads), so that the
/// title bar repainted by the change, and winit, see the new one.
fn refresh_uxtheme() {
    // SAFETY: a NUL-terminated name; uxtheme stays loaded (winit loads it too).
    let module = unsafe { LoadLibraryA(c"uxtheme.dll".as_ptr().cast()) };
    if module.is_null() {
        return;
    }
    // SAFETY: an ordinal passed as the name, as GetProcAddress allows.
    let Some(proc) = (unsafe { GetProcAddress(module, 104 as _) }) else {
        return;
    };
    // SAFETY: ordinal 104 takes nothing and returns nothing (Windows 10 1809 and later; before,
    // winit does not follow the theme at all).
    let refresh = unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "system" fn()>(proc) };
    unsafe { refresh() };
}

/// `AppsUseLightTheme` as a number, `None` when missing or not a DWORD.
fn read(key: HKEY) -> Option<u32> {
    let name = wide(VALUE);
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `name` ends with a NUL; `data` holds the 4 bytes asked for with RRF_RT_REG_DWORD.
    let read = unsafe {
        RegGetValueW(key, std::ptr::null(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&raw mut data).cast(), &mut size)
    };
    (read == ERROR_SUCCESS).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_values() {
        assert_eq!(from_registry(Some(0)), Theme::Dark);
        assert_eq!(from_registry(Some(1)), Theme::Light);
        // No value: light, Windows' default.
        assert_eq!(from_registry(None), Theme::Light);
    }

    #[test]
    #[ignore = "reads this PC's registry"]
    fn registry_live() {
        let path = wide(PATH);
        let mut key: HKEY = std::ptr::null_mut();
        assert_eq!(unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) }, ERROR_SUCCESS);
        let value = read(key);
        unsafe { RegCloseKey(key) };
        println!("{VALUE} = {value:?}");
        assert!(matches!(value, Some(0 | 1)));
    }
}
