//! Halved copies of a picture (mipmaps).

/// Half the size, each pixel the average of the 2×2 under it (the last row or column repeated
/// when odd): ~5 ms for 2048² where a filtered resize took ~60.
pub(crate) fn half(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    image::RgbaImage::from_fn((w / 2).max(1), (h / 2).max(1), |x, y| {
        let (x0, y0) = ((2 * x).min(w - 1), (2 * y).min(h - 1));
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let p = [img.get_pixel(x0, y0), img.get_pixel(x1, y0), img.get_pixel(x0, y1), img.get_pixel(x1, y1)];
        image::Rgba(std::array::from_fn(|c| ((p.iter().map(|q| q[c] as u32).sum::<u32>() + 2) / 4) as u8))
    })
}
