//! A colour from anywhere on the screen, for the editor's eyedropper. On Linux a program cannot
//! read the screen by itself under Wayland: it asks the desktop (xdg-desktop-portal,
//! `org.freedesktop.portal.Screenshot.PickColor`), which shows its own picker and answers with
//! the colour (its button and Shift+I; I picks on the image inside EyeCanSee, with a live box);
//! the same works under X11 with a portal. On Windows the pixels of the screen around
//! the pointer are read (`around_cursor`): what is drawn on the image too, and anywhere on the screen
//! when the pointer is pressed on the image and let go elsewhere (user, 2026-11-06).

/// The colour the user picked on the screen, 0-255; `Err` with why not (cancelled, no portal).
#[cfg(target_os = "linux")]
pub async fn pick() -> Result<[u8; 3], String> {
    use std::collections::HashMap;

    use futures_util::StreamExt;
    use zbus::zvariant::{OwnedValue, Structure, Value};

    let err = |e: zbus::Error| e.to_string();
    let conn = zbus::Connection::session().await.map_err(err)?;
    // The portal answers on a request object named after us and a token of ours: listened to
    // before asking, or the answer could come first.
    let token = format!("eyecansee{}", std::process::id());
    let sender = conn.unique_name().ok_or("no D-Bus name")?.trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", path, "org.freedesktop.portal.Request").await.map_err(err)?;
    let mut answers = request.receive_signal("Response").await.map_err(err)?;
    let screenshot = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Screenshot").await.map_err(err)?;
    let options: HashMap<&str, Value> = HashMap::from([("handle_token", Value::from(token.as_str()))]);
    let _: zbus::zvariant::OwnedObjectPath = screenshot.call("PickColor", &("", options)).await.map_err(err)?;
    let answer = answers.next().await.ok_or("no answer from the desktop")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = answer.body().deserialize().map_err(err)?;
    if code != 0 {
        return Err("cancelled".into());
    }
    let colour = results.get("color").ok_or("no colour in the answer")?;
    let fields = Structure::try_from(Value::try_from(colour).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let channel = |i: usize| fields.fields().get(i).and_then(|v| f64::try_from(v).ok()).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8).ok_or("not a colour");
    Ok([channel(0)?, channel(1)?, channel(2)?])
}

#[cfg(not(target_os = "linux"))]
pub async fn pick() -> Result<[u8; 3], String> {
    Err("not on this system".into())
}

/// Can the eyedropper take colours outside the image here, through the desktop's picker?
pub fn on_screen() -> bool {
    cfg!(target_os = "linux")
}

/// The `n` × `n` pixels of the screen around the pointer, row by row (Windows: one copy of
/// that square of the screen with GDI, then read at once; pixel by pixel took a long time a
/// pixel), for the eyedropper's magnifier; `None` elsewhere.
#[cfg(windows)]
pub fn around_cursor(n: usize) -> Option<Vec<[u8; 3]>> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    let (side, half) = (n as i32, (n / 2) as i32);
    let mut bgra = vec![0u8; n * n * 4];
    // SAFETY: GDI calls on the screen's device context and a memory one of our own, each object
    // freed before returning; `bgra` is as large as the 32-bit top-down bitmap asked for.
    let read = unsafe {
        if GetCursorPos(&mut p) == 0 {
            return None;
        }
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return None;
        }
        let mem = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, side, side);
        let old = SelectObject(mem, bitmap);
        let copied = BitBlt(mem, 0, 0, side, side, screen, p.x - half, p.y - half, SRCCOPY) != 0;
        SelectObject(mem, old);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: side, biHeight: -side, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..Default::default() },
            ..Default::default()
        };
        let lines = if copied { GetDIBits(mem, bitmap, 0, side as u32, bgra.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS) } else { 0 };
        DeleteObject(bitmap);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
        lines == side
    };
    read.then(|| bgra.chunks_exact(4).map(|c| [c[2], c[1], c[0]]).collect())
}

#[cfg(not(windows))]
pub fn around_cursor(_n: usize) -> Option<Vec<[u8; 3]>> {
    None
}

/// A colour as the eyedropper's box writes it: hex, RGB and HSL (hue in degrees, saturation and
/// lightness in percent).
pub fn codes([r, g, b]: [u8; 3]) -> String {
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (rf.max(gf).max(bf), rf.min(gf).min(bf));
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = match () {
        _ if d == 0.0 => 0.0,
        _ if max == rf => 60.0 * ((gf - bf) / d).rem_euclid(6.0),
        _ if max == gf => 60.0 * ((bf - rf) / d + 2.0),
        _ => 60.0 * ((rf - gf) / d + 4.0),
    };
    format!("#{r:02X}{g:02X}{b:02X}\nRGB {r}, {g}, {b}\nHSL {:.0}°, {:.0}%, {:.0}%", h, s * 100.0, l * 100.0)
}

#[cfg(test)]
mod tests {
    /// Hex, RGB and HSL of a few colours.
    #[test]
    fn colour_codes() {
        assert_eq!(super::codes([255, 0, 0]), "#FF0000\nRGB 255, 0, 0\nHSL 0°, 100%, 50%");
        assert_eq!(super::codes([30, 110, 230]), "#1E6EE6\nRGB 30, 110, 230\nHSL 216°, 80%, 51%");
        assert_eq!(super::codes([128, 128, 128]), "#808080\nRGB 128, 128, 128\nHSL 0°, 0%, 50%");
    }
}
