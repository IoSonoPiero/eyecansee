//! How large an image other programs can paste. On Linux without the Wayland data-control
//! protocol (GNOME, X11) the clipboard goes through X11, and arboard 3.7 hands the image, a PNG,
//! to the program pasting it in a single request, without the INCR transfer in pieces: the X
//! server takes about 16 MB in one request, so a larger PNG never arrives (the other program waits
//! and gets nothing). Copying it says so instead of being silent; Ctrl+V in the same window still
//! pastes it (arboard reads its own clipboard without X).

use image::ImageEncoder as _;

/// The most bytes in one X request (BIG-REQUESTS: 4 194 303 words of 4 bytes), less the 24 of the
/// ChangeProperty request around the data.
pub const LIMIT: usize = 4_194_303 * 4 - 24;

/// The size of the PNG of an RGBA image of `w` × `h`, as arboard makes it, when it is over
/// [`LIMIT`]; `None` when it fits.
pub fn too_large(rgba: &[u8], w: u32, h: u32) -> Option<usize> {
    // A PNG is never much larger than its pixels: well below the limit, it is not made.
    if rgba.len() + rgba.len() / 64 + h as usize + 4096 <= LIMIT {
        return None;
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(rgba, w, h, image::ExtendedColorType::Rgba8).ok()?;
    (png.len() > LIMIT).then_some(png.len())
}

/// Whether the clipboard goes through X11: arboard's own choice, Wayland only when the compositor
/// offers the data-control protocol (GNOME does not).
#[cfg(target_os = "linux")]
pub fn through_x11() -> bool {
    static X11: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *X11.get_or_init(|| std::env::var_os("WAYLAND_DISPLAY").is_none() || wl_clipboard_rs::utils::is_primary_selection_supported().is_err())
}

#[cfg(not(target_os = "linux"))]
pub fn through_x11() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small image is not encoded; noise of 6 MP is a PNG over the limit, a flat one is not.
    #[test]
    fn limit_of_the_png() {
        assert_eq!(too_large(&[0; 400], 10, 10), None);
        let (w, h) = (3000, 2000);
        let mut seed = 1u32;
        let noise: Vec<u8> = (0..w * h * 4)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                if i % 4 == 3 { 255 } else { (seed >> 24) as u8 }
            })
            .collect();
        assert!(too_large(&noise, w, h).is_some_and(|n| n > LIMIT));
        assert_eq!(too_large(&vec![200; (w * h * 4) as usize], w, h), None);
    }
}
