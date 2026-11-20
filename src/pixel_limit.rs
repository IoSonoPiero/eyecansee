//! The largest picture EyeCanSee decodes, checked on the size a file declares before any memory
//! is asked for it: a file of a few bytes can declare 65535 × 65535 pixels (audit 3.4, #177).

use image::ImageDecoder;

/// 512 megapixels (512 × 1024 × 1024 pixels, width × height). A real photo of 300 megapixels must
/// open (Piero, 20/11/2026); an RGBA copy at 8 bits of this many pixels takes 2 GiB. The other
/// programs stop lower: Pillow at 179 megapixels, sharp at 268, ImageMagick's policy at 128.
pub const MAX_PIXELS: u64 = 512 * 1024 * 1024;

/// Fluent id of the error, sent as `id\u{1}width × height` (`I18n::load_error` translates it).
pub const TOO_LARGE: &str = "error-image-too-large";

/// A picture of `w` × `h` pixels, refused beyond `MAX_PIXELS`.
pub fn check(w: u64, h: u64) -> Result<(), String> {
    if w.saturating_mul(h) > MAX_PIXELS { Err(format!("{TOO_LARGE}\u{1}{w} × {h}")) } else { Ok(()) }
}

/// Is `error` the one of a picture too large?
pub fn is_too_large(error: &str) -> bool {
    error.starts_with(TOO_LARGE)
}

/// A decoder of `image`, checked on its declared size before its picture is made, and allowed to
/// ask for that picture and, besides it, as much as image-rs allows by default (512 MiB, for a
/// TIFF's strips or a GIF's frame). The default alone refused TIFFs larger than 512 MiB.
pub fn checked(decoder: &mut impl ImageDecoder) -> Result<(), String> {
    let (w, h) = decoder.dimensions();
    check(w.into(), h.into())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(decoder.total_bytes().saturating_add(limits.max_alloc.unwrap_or(0)));
    decoder.set_limits(limits).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A photo of 300 megapixels passes, one pixel more than the ceiling does not, and the error
    /// says the size.
    #[test]
    fn ceiling() {
        assert!(check(20000, 15000).is_ok());
        assert!(check(MAX_PIXELS, 1).is_ok());
        assert!(check(32768, 16384).is_ok());
        let e = check(MAX_PIXELS + 1, 1).unwrap_err();
        assert!(is_too_large(&e));
        assert_eq!(e.split_once('\u{1}').unwrap().1, format!("{} × 1", MAX_PIXELS + 1));
        assert!(check(u64::MAX, u64::MAX).is_err());
        assert!(check(0, u64::MAX).is_ok());
    }
}
