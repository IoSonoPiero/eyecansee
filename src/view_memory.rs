//! A picture too large for the computer's memory in the viewer (#177). The viewer decodes a
//! picture whole, as its thumbnail (`thumb_memory::estimate`), except a JPEG, read reduced down to
//! the texture limit; then it keeps the picture shown. Over half the RAM (`thumb_memory::over_half`:
//! on a PC with 8 GB a HEIC, a 16-bit TIFF, a PSD, a JPEG XL or an OpenEXR near 512 megapixels, 5
//! to 17 GB) it is not decoded at once: in its place a card says so and offers Open anyway; the
//! arrows go on to the next picture as always (Piero, 20/11/2026). A neighbour prepared in advance
//! is never decoded: the card comes when it is the picture shown.

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
/// viewer shows a card (`big_picture_card`) instead of an error.
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

impl App {
    /// In place of a picture not opened as too large for the memory (`need`): a card that says so,
    /// with Open anyway (Piero, 20/11/2026: a card, not a window that stops the arrows; the next
    /// picture comes as always). Open anyway decodes it.
    pub(crate) fn big_picture_card(&mut self, ui: &mut egui::Ui, index: usize, need: u64) {
        let light = egui::Color32::from_gray(230);
        let text = self.i18n.t_args("view-too-large-ask", &[("need", &gigabytes(&self.i18n, need)), ("ram", &gigabytes(&self.i18n, thumb_memory::ram()))]);
        let (title, yes) = (self.i18n.t("view-too-large-title"), self.i18n.t("view-open-anyway"));
        let mut open = false;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 110.0).max(0.0));
            let card = egui::Frame::new().fill(egui::Color32::from_gray(34)).stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60))).corner_radius(10.0).inner_margin(24.0);
            card.show(ui, |ui| {
                ui.set_max_width(WIDTH);
                ui.vertical_centered(|ui| {
                    let font = egui::FontId::new(20.0, egui::FontFamily::Name(crate::BOLD.into()));
                    ui.label(egui::RichText::new(title).font(font).color(light));
                    ui.add_space(10.0);
                    ui.add(egui::Label::new(egui::RichText::new(text).color(light)).wrap());
                    ui.add_space(14.0);
                    let big = egui::Button::new(egui::RichText::new(yes).size(15.0)).min_size(egui::vec2(220.0, 34.0));
                    open = ui.add(big).clicked();
                });
            });
        });
        if open {
            self.open_anyway(index);
        }
    }

    /// The picture at `index`, too large for the memory, decoded all the same.
    fn open_anyway(&mut self, index: usize) {
        let Some(path) = self.files().get(index).map(|f| f.path.clone()) else { return };
        log!("viewer: {path} opened anyway");
        self.full.remove(&index);
        if let Some(loader) = &self.loader {
            self.pending.insert(index);
            loader.load_full(self.generation, index, path, true);
        }
    }
}

/// The width of the card: each of its sentences on one or two lines, in English and in Italian
/// (checked on a screenshot).
const WIDTH: f32 = 460.0;

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

    #[test]
    fn sizes_in_gigabytes() {
        assert_eq!(gigabytes(&crate::i18n::I18n::new("en"), 6 * GIB + GIB / 2), "6.5 GB");
        assert_eq!(gigabytes(&crate::i18n::I18n::new("it"), 8 * GIB), "8,0 GB");
    }
}
