//! A picture too large for the computer's memory in the viewer (#177). The viewer decodes a
//! picture whole, as its thumbnail (`thumb_memory::estimate`), except a JPEG, read reduced down to
//! the texture limit; then it keeps the picture shown. Over half the RAM (`thumb_memory::over_half`:
//! on a PC with 8 GB a HEIC, a 16-bit TIFF, a PSD, a JPEG XL or an OpenEXR near 512 megapixels, 5
//! to 17 GB) it is not decoded at once: the viewer asks, Cancel first, and opens it only when told
//! to (Piero, 20/11/2026); Cancel goes back to the picture shown before, or to the grid. A neighbour prepared in advance is never decoded nor asked about: the
//! question comes when it is the picture shown.

use eframe::egui;

use crate::App;
use crate::decode::MAX_SIDE;
use crate::thumb_memory::{self, FIXED, JPEG_PROGRESSIVE, JPEG_WHOLE, times};

/// Bytes per pixel shown, besides the decoding: the picture reduced to the texture limit, or made
/// RGBA (4), and the egui copy made from it (4), at the same time; its halves (a third) come after
/// the first is dropped.
const SHOWN: f64 = 8.0;

/// The memory the viewer takes to open `bytes`, file included.
pub fn estimate(bytes: &[u8], name: &str) -> u64 {
    let file = bytes.len() as u64;
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        && let Some((decoding, shown_px)) = jpeg(bytes)
    {
        return file.saturating_add(decoding).saturating_add(shown(Some(shown_px))).saturating_add(FIXED);
    }
    let px = crate::decode::header(bytes, name).map(|h| u64::from(h.size[0]) * u64::from(h.size[1]));
    thumb_memory::estimate(bytes, name).saturating_add(shown(px))
}

/// A JPEG as `decode::decode` reads it: the memory of its decoding and the pixels it gives.
/// Reduced while read (not below the texture limit) when it can be: the progressive ones keep the
/// coefficients of the whole picture meanwhile; lossless and CMYK ones are decoded whole.
fn jpeg(bytes: &[u8]) -> Option<(u64, u64)> {
    use turbojpeg::Colorspace;
    let h = turbojpeg::Decompressor::new().ok()?.read_header(bytes).ok()?;
    let px = (h.width as u64).saturating_mul(h.height as u64);
    if h.is_lossless || matches!(h.colorspace, Colorspace::CMYK | Colorspace::YCCK) {
        return Some((times(px * 3, JPEG_WHOLE), px));
    }
    let factor = crate::decode::jpeg_factor(h.width.max(h.height), MAX_SIDE, false);
    let read = factor.scale(h.width) as u64 * factor.scale(h.height) as u64;
    let coefficients = if h.is_progressive { times(px, JPEG_PROGRESSIVE) } else { 0 };
    Some((times(read * 3, JPEG_WHOLE).saturating_add(coefficients), read))
}

/// The memory of `px` pixels shown, at most the texture limit's (also when the size is unknown).
fn shown(px: Option<u64>) -> u64 {
    let most = u64::from(MAX_SIDE) * u64::from(MAX_SIDE);
    times(px.map_or(most, |px| px.min(most)), SHOWN)
}

/// The memory `bytes` needs in the viewer, when that is over half of `ram`.
pub fn too_large(bytes: &[u8], name: &str, ram: u64) -> Option<u64> {
    let need = estimate(bytes, name);
    thumb_memory::over_half(need, ram).then_some(need)
}

/// Before the viewer decodes `bytes`: the error of a picture `too_large` for `ram`, unless it is
/// opened `anyway`. Checked for the neighbours prepared in advance too: they are not decoded.
pub fn guard(bytes: &[u8], name: &str, anyway: bool, ram: u64) -> Result<(), String> {
    match too_large(bytes, name, ram) {
        Some(need) if !anyway => {
            log!("viewer: {name} not opened, {} MiB is over half the RAM", need >> 20);
            Err(error(need))
        }
        _ => Ok(()),
    }
}

/// The error of a picture not opened because it is `too_large`, with the memory it needs: the
/// viewer asks about it (`question`) instead of showing an error.
const TOO_LARGE: &str = "viewer: picture too large for the memory, bytes needed: ";

pub fn error(need: u64) -> String {
    format!("{TOO_LARGE}{need}")
}

/// The memory needed, if `e` is the error of a picture `too_large`.
pub fn need_of(e: &str) -> Option<u64> {
    e.strip_prefix(TOO_LARGE)?.parse().ok()
}

/// 6.4 GB (as `human::size`, 1024-based, the language's decimal separator).
pub fn gigabytes(i18n: &crate::i18n::I18n, bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / f64::from(1u32 << 30)).replace('.', &i18n.t("decimal-separator"))
}

/// The memory the picture shown, `current`, needs, when the viewer asks whether to open it: it
/// was not opened as too large.
fn question<T>(loaded: Option<&Result<T, String>>) -> Option<u64> {
    match loaded {
        Some(Err(e)) => need_of(e),
        _ => None,
    }
}

/// The picture to go back to on Cancel, at a frame without the question: the one `viewing` once it
/// is `shown` (not while it loads: it may be the large one itself), the grid when no picture is.
fn remembered(before: Option<usize>, viewing: Option<usize>, shown: bool) -> Option<usize> {
    match viewing {
        None => None,
        Some(_) if shown => viewing,
        Some(_) => before,
    }
}

/// Where the viewer goes on Cancel: the picture shown `before` the one `asked` about, if there is
/// still one there; `None`, back to the grid, as after an error.
fn after_cancel(before: Option<usize>, asked: usize, files: usize) -> Option<usize> {
    before.filter(|&b| b != asked && b < files)
}

impl App {
    /// The picture shown and the memory it needs, while the viewer asks whether to open it.
    pub(crate) fn big_question(&self) -> Option<(usize, u64)> {
        let current = self.viewing?;
        question(self.full.get(&current)).map(|need| (current, need))
    }

    /// The question: Open anyway, or Cancel (the default, also with Enter and Esc), which goes
    /// back to the picture shown before, or to the grid. Asked again when the picture is shown again.
    pub(crate) fn big_picture_window(&mut self, ctx: &egui::Context) {
        let Some((index, need)) = self.big_question() else {
            let shown = self.viewing.is_some_and(|v| matches!(self.full.get(&v), Some(Ok(_))));
            self.viewer_before = remembered(self.viewer_before, self.viewing, shown);
            return;
        };
        let name = self.files().get(index).map(|f| crate::source::shown(&f.path).to_string()).unwrap_or_default();
        let text = self.i18n.t_args("view-too-large-ask", &[("file", &name), ("need", &gigabytes(&self.i18n, need)), ("ram", &gigabytes(&self.i18n, thumb_memory::ram()))]);
        let (title, yes, no) = (self.i18n.t("view-too-large-title"), self.i18n.t("view-open-anyway"), self.i18n.t("delete-no"));
        let keys = self.delete_answer.take().map(|_| false);
        match crate::confirm::yes_no_cancel_first(ctx, "big-picture", WIDTH, (title, text), yes, true, no, keys) {
            Some(true) => {
                log!("viewer: {name} opened anyway, {} MiB", need >> 20);
                self.full.remove(&index);
                let path = self.files().get(index).map(|f| f.path.clone());
                if let (Some(loader), Some(path)) = (&self.loader, path) {
                    self.pending.insert(index);
                    loader.load_full(self.generation, index, path, true);
                }
            }
            Some(false) => match after_cancel(self.viewer_before, index, self.files().len()) {
                Some(before) => self.viewing = Some(before),
                None => self.close_viewer(),
            },
            None => {}
        }
    }
}

/// The width of the question: each of its two sentences on one line for a short file name, in
/// English and in Italian (checked on a screenshot).
const WIDTH: f32 = 540.0;

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let pixels = turbojpeg::Image { pixels: img.as_raw().as_slice(), width: w as usize, pitch: w as usize * 3, height: h as usize, format: turbojpeg::PixelFormat::RGB };
        turbojpeg::Compressor::new().unwrap().compress_to_vec(pixels).unwrap()
    }

    /// A JPEG larger than a texture is counted as read reduced (to 9000 × 6000 here, 6/8), one
    /// within it whole; and either, besides, what is shown.
    #[test]
    fn jpeg_read_reduced() {
        let big = jpeg_bytes(12000, 8000);
        let read = 9000 * 6000;
        let shown_px = read;
        assert_eq!(estimate(&big, "a.jpg"), big.len() as u64 + times(read * 3, JPEG_WHOLE) + times(shown_px, SHOWN) + FIXED);
        let small = jpeg_bytes(1200, 800);
        assert_eq!(estimate(&small, "a.jpg"), small.len() as u64 + times(960_000 * 3, JPEG_WHOLE) + times(960_000, SHOWN) + FIXED);
    }

    /// Any other picture as its thumbnail, decoded whole, and what is shown, at most a texture's.
    #[test]
    fn others_as_their_thumbnail() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb16(1000, 500).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        assert_eq!(estimate(&png, "a.png"), thumb_memory::estimate(&png, "a.png") + times(500_000, SHOWN));
        let unknown = vec![0u8; 1000];
        assert_eq!(estimate(&unknown, "a.xcf"), thumb_memory::estimate(&unknown, "a.xcf") + times(8192 * 8192, SHOWN));
    }

    /// Over half the RAM it is not opened at once, and the error says how much it needs.
    #[test]
    fn threshold_and_error() {
        let png = {
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(100, 100).write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let need = estimate(&png, "a.png");
        assert_eq!(too_large(&png, "a.png", 2 * need), None, "exactly half");
        assert_eq!(too_large(&png, "a.png", 2 * need - 2), Some(need));
        assert_eq!(too_large(&png, "a.png", 8 * GIB), None);
        assert_eq!(need_of(&error(need)), Some(need));
        assert_eq!(need_of("cannot read"), None);
        assert_eq!(need_of(crate::thumb_memory::TOO_LARGE), None);
        assert_eq!(guard(&png, "a.png", false, 2 * need - 2), Err(error(need)), "the shown one and the neighbours alike");
        assert_eq!(guard(&png, "a.png", true, 2 * need - 2), Ok(()), "Open anyway");
        assert_eq!(guard(&png, "a.png", false, 2 * need), Ok(()));
    }

    /// Asked only about the picture shown, when it was not opened as too large (a neighbour not
    /// opened is only in `full`: `big_question` looks at the picture shown alone).
    #[test]
    fn asked_only_when_not_opened() {
        let big: Result<(), String> = Err(error(5 * GIB));
        let other: Result<(), String> = Err("damaged".into());
        assert_eq!(question(Some(&big)), Some(5 * GIB));
        assert_eq!(question(Some(&other)), None);
        assert_eq!(question(Some(&Ok(()))), None);
        assert_eq!(question::<()>(None), None, "still loading");
    }

    /// Cancel goes back to the picture shown before, else to the grid.
    #[test]
    fn cancel_goes_back() {
        assert_eq!(after_cancel(Some(2), 3, 10), Some(2));
        assert_eq!(after_cancel(None, 3, 10), None, "opened from the grid");
        assert_eq!(after_cancel(Some(3), 3, 10), None);
        assert_eq!(after_cancel(Some(12), 3, 10), None, "no longer in the folder");
        // Shown 2, then 3 still loading, then 3 is the large one: back to 2.
        let before = remembered(None, Some(2), true);
        let before = remembered(before, Some(3), false);
        assert_eq!(after_cancel(before, 3, 10), Some(2));
        assert_eq!(remembered(Some(2), None, false), None, "in the grid");
    }

    #[test]
    fn sizes_in_gigabytes() {
        assert_eq!(gigabytes(&crate::i18n::I18n::new("en"), 6 * GIB + GIB / 2), "6.5 GB");
        assert_eq!(gigabytes(&crate::i18n::I18n::new("it"), 8 * GIB), "8,0 GB");
    }
}
