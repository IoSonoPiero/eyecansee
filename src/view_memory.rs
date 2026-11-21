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
use crate::source::Store;
use crate::thumb_memory::{self, Estimate, FIXED, JPEG_PROGRESSIVE, JPEG_WHOLE, times};

/// Bytes per pixel shown, besides the decoding: the picture reduced to the texture limit, or made
/// RGBA (4), and the egui copy made from it (4), at the same time; its halves (a third) come after
/// the first is dropped.
const SHOWN: f64 = 8.0;

/// What a picture is opened for: the viewer, which reads a JPEG reduced to the texture limit, or
/// a decoding at full resolution (`decode::decode_full`: the editor, Save as, Convert, Resize).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Use {
    View,
    Whole,
}

/// The memory `bytes` (the start of a file of `size` bytes, or all of it) take opened for `what`,
/// file included: the decoding, as the thumbnail's (`thumb_memory::estimate_sized`, which decodes
/// whole every picture but a JPEG), and the picture shown or its preview, at most a texture.
/// What comes after the decoding (the editor's copies, the encoding of a new file) is not counted.
pub fn estimate(bytes: &[u8], size: u64, name: &str, what: Use) -> Estimate {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        && let Some((decoding, shown_px)) = jpeg(bytes, what)
    {
        return Estimate::from_header(size.saturating_add(decoding).saturating_add(shown(Some(shown_px))).saturating_add(FIXED));
    }
    thumb_memory::estimate_sized(bytes, size, name).plus(shown(shown_pixels(bytes, name)))
}

/// The pixels of the picture shown, when its header tells them the same from the start of its
/// file as from all of it: a RAW other than DNG says them only with its sensor data (and rawler
/// sets aside its memory first: not read for this), an SVG is shown at `SVG_SIDE`.
fn shown_pixels(bytes: &[u8], name: &str) -> Option<u64> {
    if crate::decode::is_raw(name) {
        crate::raw_meta::dng_pixels(bytes)
    } else if crate::decode::is_svg(name) {
        Some((crate::decode::SVG_SIDE as u64).pow(2))
    } else {
        crate::decode::header(bytes, name).map(|h| u64::from(h.size[0]) * u64::from(h.size[1]))
    }
}

/// A JPEG as `decode::decode` reads it for the viewer, or whole: the memory of its decoding and
/// the pixels it gives. Reduced while read (not below the texture limit) when it can be: the
/// progressive ones keep the coefficients of the whole picture meanwhile; lossless and CMYK ones
/// are decoded whole.
fn jpeg(bytes: &[u8], what: Use) -> Option<(u64, u64)> {
    use turbojpeg::Colorspace;
    let h = turbojpeg::Decompressor::new().ok()?.read_header(bytes).ok()?;
    let px = (h.width as u64).saturating_mul(h.height as u64);
    let coefficients = if h.is_progressive { times(px, JPEG_PROGRESSIVE) } else { 0 };
    if what == Use::Whole {
        return Some((times(px * 3, JPEG_WHOLE).saturating_add(coefficients), px));
    }
    if h.is_lossless || matches!(h.colorspace, Colorspace::CMYK | Colorspace::YCCK) {
        return Some((times(px * 3, JPEG_WHOLE), px));
    }
    let factor = crate::decode::jpeg_factor(h.width.max(h.height), MAX_SIDE, false);
    let read = factor.scale(h.width) as u64 * factor.scale(h.height) as u64;
    Some((times(read * 3, JPEG_WHOLE).saturating_add(coefficients), read))
}

/// The memory of `px` pixels shown, at most the texture limit's (also when the size is unknown).
fn shown(px: Option<u64>) -> u64 {
    let most = u64::from(MAX_SIDE) * u64::from(MAX_SIDE);
    times(px.map_or(most, |px| px.min(most)), SHOWN)
}

/// How much of a file over `ONCE` is read first, to tell from its header whether it is too large
/// for the memory: the size of a JPEG, a HEIC, a PNG or a PSD is in it.
pub const HEAD: u64 = 256 * 1024;

/// Up to this size a file is read once, whole, and its size is told from the bytes in memory
/// before it is decoded (Piero, 21/11/2026): on a phone or a server a second request for the
/// start cost more than the bytes it saved. Above it, the start first (`HEAD`).
pub const ONCE: u64 = 32 << 20;

/// Why `read_unless_too_large` gave no bytes.
#[derive(Debug, PartialEq)]
pub enum NotRead {
    /// Too large for the memory: what it needs.
    TooLarge(u64),
    Failed(crate::source::Error),
}

/// The bytes of `path` (`size` bytes, if known), unless `too_large` finds them too large for the
/// memory, before anything is decoded. A file up to `ONCE` is read once, whole; of a larger one
/// the start first (`HEAD` bytes), and the rest is not read when that is enough
/// (`thumb_memory::Estimate::sure`, Piero, 20/11/2026), else it is read whole and looked at again.
/// `too_large(bytes, size)`: what they need, when too much.
pub async fn read_unless_too_large(op: &Store, path: &str, size: Option<u64>, too_large: impl Fn(&[u8], u64) -> Option<u64>) -> Result<Vec<u8>, NotRead> {
    read_within(op, path, size, ONCE, too_large).await
}

/// `read_unless_too_large`, a file up to `once` bytes read whole at once.
async fn read_within(op: &Store, path: &str, size: Option<u64>, once: u64, too_large: impl Fn(&[u8], u64) -> Option<u64>) -> Result<Vec<u8>, NotRead> {
    // A range past the end is an error (OpenDAL's "RangeNotSatisfied"): the size first.
    let size = match size {
        Some(s) => s,
        None => op.size(path).await.map_err(NotRead::Failed)?,
    };
    // A start that cannot be read (the file shorter now than listed): the whole file says.
    if size > once.max(HEAD)
        && let Ok(head) = op.read_range(path, 0, HEAD).await
        && let Some(need) = too_large(&head, size)
    {
        log!("{path}: {} MiB, over half the RAM, told by its first {} bytes: not read further", need >> 20, head.len());
        return Err(NotRead::TooLarge(need));
    }
    let bytes = op.read(path).await.map_err(NotRead::Failed)?;
    match too_large(&bytes, bytes.len() as u64) {
        Some(need) => {
            log!("{path}: {} MiB, over half the RAM, told by its {} bytes read whole", need >> 20, bytes.len());
            Err(NotRead::TooLarge(need))
        }
        None => Ok(bytes),
    }
}

impl NotRead {
    /// As an error: `error` for a picture too large, `failed` of the error of a read.
    pub fn message(self, failed: impl FnOnce(crate::source::Error) -> String) -> String {
        match self {
            NotRead::TooLarge(need) => error(need),
            NotRead::Failed(e) => failed(e),
        }
    }
}

/// The bytes of the picture `path` (`size` bytes, if known) to open for `what`, unless it is too
/// large for the memory and not opened `anyway`: the viewer shows a card for it, the pages ask
/// about it. 3D models, drawings and videos are read as they are: they have limits of their own.
pub async fn read(op: &Store, path: &str, size: Option<u64>, what: Use, anyway: bool) -> Result<Vec<u8>, NotRead> {
    read_for(op, path, size, (what, anyway), (thumb_memory::ram(), ONCE)).await
}

/// `read` on a computer with `ram`, a file up to `once` bytes read whole at once.
async fn read_for(op: &Store, path: &str, size: Option<u64>, (what, anyway): (Use, bool), (ram, once): (u64, u64)) -> Result<Vec<u8>, NotRead> {
    if anyway || crate::model::is_model(path) || crate::cad::is_cad(path) || crate::decode::is_played(path) {
        return op.read(path).await.map_err(NotRead::Failed);
    }
    read_within(op, path, size, once, |bytes, size| estimate(bytes, size, path, what).over_half(bytes.len(), size, path, ram)).await
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
            loader.load_full(self.generation, index, path, None, true);
        }
    }
}

/// The memory a page's picture (the editor's, Save as's) needs, while it was not read as too
/// large: the page asks whether to open it anyway (`big_picture_ask`).
pub(crate) fn asked<T>(original: Option<&Result<T, String>>) -> Option<u64> {
    match original {
        Some(Err(e)) => need_of(e),
        _ => None,
    }
}

impl App {
    /// Over a page (the editor with `edit`, Save as…) whose picture was not read as too large for
    /// the memory, the question: Open anyway reads it again whatever it needs, Cancel closes the
    /// page (`false`).
    pub(crate) fn big_picture_page<T>(&mut self, ctx: &egui::Context, original: &mut Option<Result<T, String>>, path: &str, edit: bool) -> bool {
        let Some(need) = asked(original.as_ref()) else { return true };
        match self.big_picture_ask(ctx, need) {
            Some(true) => {
                *original = None;
                if let Some(loader) = &self.loader {
                    loader.load_original(path.to_string(), edit, true);
                }
                true
            }
            Some(false) => false,
            None => true,
        }
    }

    /// The question of the editor and of Save as… on a picture too large for the memory (`need`):
    /// `Some(true)` Open anyway, `Some(false)` Cancel, the default (also Enter and Esc), which
    /// closes the page; `None` while it is open. The user asked for something on that picture:
    /// a window, unlike the viewer's card (Piero, 20/11/2026).
    fn big_picture_ask(&mut self, ctx: &egui::Context, need: u64) -> Option<bool> {
        let text = self.i18n.t_args("view-too-large-ask", &[("need", &gigabytes(&self.i18n, need)), ("ram", &gigabytes(&self.i18n, thumb_memory::ram()))]);
        let (title, yes, no) = (self.i18n.t("view-too-large-title"), self.i18n.t("view-open-anyway"), self.i18n.t("delete-no"));
        let keys = self.delete_answer.take().map(|_| false);
        let answer = crate::confirm::yes_no_cancel_first(ctx, "big-picture", WIDTH, (title, text), yes, true, no, keys);
        if let Some(open) = answer {
            log!("picture too large for the memory, {} MiB: {}", need >> 20, if open { "Open anyway" } else { "Cancel" });
        }
        answer
    }
}

/// The width of the card and of the question: each of their sentences on one or two lines, in
/// English and in Italian (checked on a screenshot).
const WIDTH: f32 = 560.0;

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let pixels = turbojpeg::Image { pixels: img.as_raw().as_slice(), width: w as usize, pitch: w as usize * 3, height: h as usize, format: turbojpeg::PixelFormat::RGB };
        turbojpeg::Compressor::new().unwrap().compress_to_vec(pixels).unwrap()
    }

    fn need(bytes: &[u8], name: &str, what: Use) -> u64 {
        estimate(bytes, bytes.len() as u64, name, what).need
    }

    /// The memory a whole file `bytes` needs opened for `what`, when that is over half of `ram`.
    fn too_large(bytes: &[u8], name: &str, ram: u64, what: Use) -> Option<u64> {
        estimate(bytes, bytes.len() as u64, name, what).over_half(bytes.len(), bytes.len() as u64, name, ram)
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(w, h).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// A JPEG larger than a texture is counted as read reduced (to 9000 × 6000 here, 6/8), one
    /// within it whole; and either, besides, what is shown. Decoded at full resolution (editor,
    /// Save as, Convert, Resize), always whole.
    #[test]
    fn jpeg_read_reduced() {
        let big = jpeg_bytes(12000, 8000);
        let read = 9000 * 6000;
        let shown_px = read;
        assert_eq!(need(&big, "a.jpg", Use::View), big.len() as u64 + times(read * 3, JPEG_WHOLE) + times(shown_px, SHOWN) + FIXED);
        let small = jpeg_bytes(1200, 800);
        assert_eq!(need(&small, "a.jpg", Use::View), small.len() as u64 + times(960_000 * 3, JPEG_WHOLE) + times(960_000, SHOWN) + FIXED);
        let whole = 12000 * 8000;
        assert_eq!(need(&big, "a.jpg", Use::Whole), big.len() as u64 + times(whole * 3, JPEG_WHOLE) + times(8192 * 8192, SHOWN) + FIXED);
    }

    /// Any other picture as its thumbnail, decoded whole, and what is shown, at most a texture's;
    /// the same at full resolution.
    #[test]
    fn others_as_their_thumbnail() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb16(1000, 500).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        assert_eq!(need(&png, "a.png", Use::View), thumb_memory::estimate(&png, "a.png") + times(500_000, SHOWN));
        assert_eq!(need(&png, "a.png", Use::Whole), need(&png, "a.png", Use::View));
        let unknown = vec![0u8; 1000];
        assert_eq!(need(&unknown, "a.xcf", Use::View), thumb_memory::estimate(&unknown, "a.xcf") + times(8192 * 8192, SHOWN));
    }

    /// Over half the RAM it is not opened at once, and the error says how much it needs.
    #[test]
    fn threshold_and_error() {
        let png = png_bytes(100, 100);
        let need = need(&png, "a.png", Use::View);
        assert_eq!(too_large(&png, "a.png", 2 * need, Use::View), None, "exactly half");
        assert_eq!(too_large(&png, "a.png", 2 * need - 2, Use::View), Some(need));
        assert_eq!(too_large(&png, "a.png", 8 * GIB, Use::View), None);
        assert_eq!(need_of(&error(need)), Some(need));
        assert_eq!(need_of("cannot read"), None);
        assert_eq!(need_of(crate::thumb_memory::TOO_LARGE), None);
        assert_eq!(NotRead::TooLarge(need).message(|_| String::new()), error(need));
        assert_eq!(NotRead::Failed(("error-detail", "gone".into())).message(|(_, d)| d), "gone");
    }

    /// How many bytes `read_within` reads of `name` in `dir`, for a computer with `ram`, a file up
    /// to `once` bytes read at once, and what it gives back (the length of the bytes, or the
    /// memory needed).
    fn bytes_read(dir: &std::path::Path, name: &str, size: Option<u64>, (ram, once): (u64, u64)) -> (u64, Result<usize, u64>) {
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        crate::source::BYTES_READ.with(|n| n.set(0));
        let too_large = |bytes: &[u8], size| estimate(bytes, size, name, Use::View).over_half(bytes.len(), size, name, ram);
        let r = rt.block_on(read_within(&op, name, size, once, too_large));
        let r = r.map(|b| b.len()).map_err(|e| match e {
            NotRead::TooLarge(need) => need,
            NotRead::Failed(e) => panic!("{e:?}"),
        });
        (crate::source::BYTES_READ.with(std::cell::Cell::get), r)
    }

    /// Over `ONCE` (here made small), a picture too large is told from the first `HEAD` bytes
    /// when they hold its header, or when its estimate goes by the size alone (a RAW of 230 MB, of
    /// bytes at random, as `huge.cr3`): the rest is not read, with or without the size from the
    /// listing. A TIFF whose tags come after the pixels is read whole first; a picture not too
    /// large is read whole, after its start.
    #[test]
    fn only_the_header_of_one_too_large() {
        let dir = std::env::temp_dir().join(format!("view-memory-head-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let raw = std::fs::File::create(dir.join("huge.cr3")).unwrap();
        raw.set_len(230 << 20).unwrap();
        drop(raw);
        let ram = 8 * GIB;
        for size in [Some(230 << 20), None] {
            let (read, r) = bytes_read(&dir, "huge.cr3", size, (ram, ONCE));
            assert_eq!(read, HEAD, "{size:?}");
            assert!(r.is_err_and(|need| need > (230 * 72) << 20), "{size:?}");
        }
        // A PNG of noise, larger than `HEAD` (its header in the first bytes), on a computer
        // with less than twice what it needs.
        let png = noise_png();
        std::fs::write(dir.join("big.png"), &png).unwrap();
        let (read, r) = bytes_read(&dir, "big.png", Some(png.len() as u64), (2 * need(&png, "a.png", Use::View) - 2, 0));
        assert_eq!((read, r.is_err()), (HEAD, true));
        // The same with plenty of memory: its start, then all of it.
        let (read, r) = bytes_read(&dir, "big.png", Some(png.len() as u64), (64 * GIB, 0));
        assert_eq!((read, r), (HEAD + png.len() as u64, Ok(png.len())));
        // A TIFF of 1000 × 1000 whose tags come after its pixels: its start does not say.
        let tif = tags_after_pixels();
        std::fs::write(dir.join("a.tif"), &tif).unwrap();
        let tiny = 2 * need(&tif, "a.tif", Use::View) - 2;
        let (read, r) = bytes_read(&dir, "a.tif", Some(tif.len() as u64), (tiny, 0));
        assert_eq!((read, r.is_err()), (HEAD + tif.len() as u64, true), "read whole, then refused");
        // A small file is read once, whole.
        std::fs::write(dir.join("small.png"), png_bytes(10, 10)).unwrap();
        let (read, r) = bytes_read(&dir, "small.png", None, (2 * GIB, 0));
        assert_eq!(Ok(read as usize), r);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A PNG of noise of 400 × 400, larger than `HEAD`, its header in the first bytes.
    fn noise_png() -> Vec<u8> {
        let noise = image::RgbImage::from_fn(400, 400, |x, y| image::Rgb(((x * 400 + y).wrapping_mul(2654435761) >> 8).to_le_bytes()[..3].try_into().unwrap()));
        let mut png = std::io::Cursor::new(Vec::new());
        noise.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        assert!(png.len() as u64 > HEAD, "{}", png.len());
        png
    }

    /// A TIFF of 1000 × 1000 whose tags come after its pixels, past `HEAD`.
    fn tags_after_pixels() -> Vec<u8> {
        let mut tif = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1000, 1000).write_to(&mut tif, image::ImageFormat::Tiff).unwrap();
        let tif = tif.into_inner();
        let offset = u32::from_le_bytes(tif[4..8].try_into().unwrap());
        assert!(u64::from(offset) > HEAD, "the tags at {offset}");
        tif
    }

    /// Up to `ONCE`, a file is read once, whole, refused or not (Piero, 21/11/2026): a PNG or a
    /// TIFF whose tags come after its pixels alike, larger than `HEAD`. A file of 68 bytes that
    /// declares 60000 × 60000 pixels is still refused from its bytes in memory, before decoding.
    #[test]
    fn read_once_up_to_32_mb() {
        let dir = std::env::temp_dir().join(format!("view-memory-once-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in [("big.png", noise_png()), ("a.tif", tags_after_pixels())] {
            std::fs::write(dir.join(name), &bytes).unwrap();
            let len = bytes.len() as u64;
            let tiny = 2 * need(&bytes, name, Use::View) - 2;
            for size in [Some(len), None] {
                assert_eq!(bytes_read(&dir, name, size, (tiny, ONCE)), (len, Err(need(&bytes, name, Use::View))), "{name} {size:?}");
                assert_eq!(bytes_read(&dir, name, size, (64 * GIB, ONCE)), (len, Ok(len as usize)), "{name} {size:?}");
            }
        }
        let declared = crate::decode::tests::audit_png(60000, 60000);
        assert_eq!(declared.len(), 68);
        std::fs::write(dir.join("declared.png"), &declared).unwrap();
        let (read, r) = bytes_read(&dir, "declared.png", None, (8 * GIB, ONCE));
        assert!(read == 68 && r.is_err_and(|need| need > 10 * GIB), "{read} {r:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The viewer (the picture shown and its neighbours alike), the editor, Save as, Convert and
    /// Resize: a picture too large is refused before it is decoded (over `ONCE`, from the start of
    /// its file), unless opened anyway;
    /// a 3D model or a drawing is never refused here (limits of their own).
    #[test]
    fn read_refuses_unless_anyway() {
        let dir = std::env::temp_dir().join(format!("view-memory-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let size = 1 << 20;
        for name in ["a.cr3", "a.glb", "a.dxf"] {
            std::fs::File::create(dir.join(name)).unwrap().set_len(size).unwrap();
        }
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        // 72 MiB and more needed for a RAW of 1 MiB: over half of 64 MiB.
        let ram = 64 << 20;
        let read = |name: &str, what, anyway, once| {
            crate::source::BYTES_READ.with(|n| n.set(0));
            let r = rt.block_on(read_for(&op, name, None, (what, anyway), (ram, once))).map(|b| b.len() as u64);
            (r, crate::source::BYTES_READ.with(std::cell::Cell::get))
        };
        for what in [Use::View, Use::Whole] {
            // Over `ONCE` (here 0), from the start; up to it, from the whole file.
            for (once, read_first) in [(0, HEAD), (ONCE, size)] {
                let (r, bytes) = read("a.cr3", what, false, once);
                assert!(matches!(r, Err(NotRead::TooLarge(_))) && bytes == read_first, "{what:?}: {r:?}, {bytes}");
                assert_eq!(read("a.cr3", what, true, once), (Ok(size), size), "{what:?}: Open anyway");
                for other in ["a.glb", "a.dxf"] {
                    assert_eq!(read(other, what, false, once), (Ok(size), size), "{other}");
                }
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// From the start of a file or from all of it, the same estimate when it is sure: a RAW
    /// whose size goes by the file, an SVG shown at its own side.
    #[test]
    fn same_from_the_start_and_whole() {
        let size = 50 << 20;
        let head = vec![0x33u8; HEAD as usize];
        let whole = vec![0x33u8; 2 * HEAD as usize];
        for name in ["a.cr3", "a.nef", "a.svg"] {
            for what in [Use::View, Use::Whole] {
                assert_eq!(estimate(&head, size, name, what).need, estimate(&whole, size, name, what).need, "{name} {what:?}");
            }
        }
    }

    /// The editor and Save as ask only about a picture not read as too large (not while it is
    /// read, nor for another error).
    #[test]
    fn pages_ask_only_when_too_large() {
        let big: Result<(), String> = Err(error(5 * GIB));
        let other: Result<(), String> = Err("damaged".into());
        assert_eq!(asked(Some(&big)), Some(5 * GIB));
        assert_eq!(asked(Some(&other)), None);
        assert_eq!(asked(Some(&Ok(()))), None);
        assert_eq!(asked::<()>(None), None, "still read");
    }

    #[test]
    fn sizes_in_gigabytes() {
        assert_eq!(gigabytes(&crate::i18n::I18n::new("en"), 6 * GIB + GIB / 2), "6.5 GB");
        assert_eq!(gigabytes(&crate::i18n::I18n::new("it"), 8 * GIB), "8,0 GB");
    }
}
