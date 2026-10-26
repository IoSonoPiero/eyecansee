//! Converting and resizing several images at once, without the interface: what each image
//! becomes (`convert`), and the whole job in the background (`run`). The originals are never
//! touched: the new files get free names, next to them, in a subfolder or in another folder,
//! also on another device.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use image::imageops::FilterType;

use crate::encode::{self, Format, Options};
use crate::source::{self, Store};
use crate::transform::Turn;

/// New size of the images.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Size {
    Same,
    /// Percent of the original sides.
    Percent(u32),
    /// Inside this box, keeping the proportions.
    Fit(u32, u32),
}

/// How the pixels are worked out when the size changes, from the sharpest to the plainest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    Lanczos3,
    CatmullRom,
    Gaussian,
    Bilinear,
    /// No smoothing: for pixel art.
    Nearest,
}

impl Filter {
    pub const ALL: [Filter; 5] = [Filter::Lanczos3, Filter::CatmullRom, Filter::Gaussian, Filter::Bilinear, Filter::Nearest];

    /// Fluent id of the name.
    pub fn label(self) -> &'static str {
        match self {
            Filter::Lanczos3 => "batch-filter-lanczos",
            Filter::CatmullRom => "batch-filter-catmull",
            Filter::Gaussian => "batch-filter-gaussian",
            Filter::Bilinear => "batch-filter-bilinear",
            Filter::Nearest => "batch-filter-nearest",
        }
    }

    fn kind(self) -> FilterType {
        match self {
            Filter::Lanczos3 => FilterType::Lanczos3,
            Filter::CatmullRom => FilterType::CatmullRom,
            Filter::Gaussian => FilterType::Gaussian,
            Filter::Bilinear => FilterType::Triangle,
            Filter::Nearest => FilterType::Nearest,
        }
    }
}

/// What to do with every image.
#[derive(Clone, PartialEq, Debug)]
pub struct Job {
    /// `None`: the format of each original, written as "Save" writes it.
    pub format: Option<Format>,
    pub options: Options,
    pub size: Size,
    pub filter: Filter,
    /// Flips and quarter turns, before the size changes.
    pub turn: Turn,
    /// Images smaller than the size asked for are made larger too.
    pub enlarge: bool,
}

impl Default for Job {
    fn default() -> Self {
        Job { format: Some(Format::Jpeg), options: Options::default(), size: Size::Same, filter: Filter::Lanczos3, turn: Turn::default(), enlarge: false }
    }
}

impl Job {
    /// Same format, same size, not turned: every image would only be encoded again, losing quality.
    pub fn does_nothing(&self) -> bool {
        self.format.is_none() && self.size == Size::Same && self.turn.is_none()
    }
}

/// Size of an image of `w` × `h` pixels after the job; never below 1 pixel.
pub fn target(w: u32, h: u32, size: Size, enlarge: bool) -> (u32, u32) {
    let scale = match size {
        Size::Same => return (w, h),
        Size::Percent(p) => p as f64 / 100.0,
        Size::Fit(bw, bh) => (bw as f64 / w as f64).min(bh as f64 / h as f64),
    };
    let scale = if enlarge { scale } else { scale.min(1.0) };
    let side = |v: u32| ((v as f64 * scale).round() as u32).max(1);
    (side(w), side(h))
}

/// Why an image was left out (Fluent ids), with its name.
pub const SKIP_FORMAT: &str = "batch-skip-format";
pub const SKIP_INSIDE: &str = "batch-skip-inside";

/// The file `bytes` (named `name`) after the job: the new name, without the folder, and its
/// content. `Err((id, detail))`: a Fluent id with the name or the error.
pub fn convert(bytes: &[u8], name: &str, job: &Job) -> Result<(String, Vec<u8>), (&'static str, String)> {
    if job.format.is_none() && !encode::saves_back(name) {
        return Err((SKIP_FORMAT, name.to_string()));
    }
    // Only turned, in its own format: as "Save" does it, a JPEG without losing quality when
    // its size allows it.
    if job.format.is_none()
        && job.size == Size::Same
        && (encode::can_turn_in_place(bytes, name) || crate::ultrahdr::is(bytes))
        && let Ok((data, _)) = encode::turned_file(bytes, name, job.turn)
    {
        return Ok((name.to_string(), data));
    }
    let mut image = crate::decode::decode_full(bytes, name).map_err(|e| ("batch-failed", format!("{name}: {e}")))?;
    // The other frames of an animation, the other pages would be lost. After decoding, so that
    // a file that is not an image says so. A phone JPEG (HDR gain map, motion photo video) is
    // not left out: the new file is the picture, the original keeps the rest.
    if Format::of_file(name).is_some_and(|f| f != Format::Jpeg) && !encode::can_turn_in_place(bytes, name) {
        return Err((SKIP_INSIDE, name.to_string()));
    }
    job.turn.apply(&mut image);
    let (w, h) = target(image.width(), image.height(), job.size, job.enlarge);
    if (w, h) != (image.width(), image.height()) {
        image = image.resize_exact(w, h, job.filter.kind());
    }
    let failed = |e: String| ("batch-failed", format!("{name}: {e}"));
    match job.format {
        // As "Save" writes them (which would refuse a phone JPEG: it writes over the original).
        None if Format::of_file(name) == Some(Format::Jpeg) => {
            let options = Options { jpeg_quality: 95, ..Options::default() };
            Ok((name.to_string(), encode::Original::with_meta(image, bytes).encode(&options).map_err(failed)?))
        }
        None => encode::encoded_back(bytes, name, image).map(|(data, _)| (name.to_string(), data)).map_err(|_| (SKIP_INSIDE, name.to_string())),
        Some(format) => {
            let options = Options { format, turn: Turn::default(), ..job.options };
            let data = encode::Original::with_meta(image, bytes).encode(&options).map_err(failed)?;
            Ok((with_ext(name, format), data))
        }
    }
}

/// `name` with the extension of `format` in place of its own.
pub fn with_ext(name: &str, format: Format) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    format!("{stem}.{}", format.ext())
}

/// How the job is going, read by the window; `cancel` stops it after the image under way.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub current: Mutex<String>,
    pub cancel: AtomicBool,
    /// Paths of the new files.
    pub saved: Mutex<Vec<String>>,
    /// Images left out or failed: Fluent id and detail.
    pub failed: Mutex<Vec<(&'static str, String)>>,
    pub finished: AtomicBool,
}

/// Where the new files go: a folder of a source, made first if `create`.
pub struct Dest {
    pub op: Store,
    pub dir: String,
    pub create: bool,
}

/// Every image of `paths` (read from `op`) after `job`, written into `to` with a free name;
/// never over a file.
pub async fn run(op: &Store, paths: Vec<String>, to: Dest, job: Job, p: &Progress) {
    p.total.store(paths.len(), Ordering::Relaxed);
    let Dest { op: out, dir: dest, create } = to;
    if create
        && let Err((id, detail)) = out.create_dir(&dest).await
    {
        p.failed.lock().unwrap().push((id, detail));
        p.finished.store(true, Ordering::Relaxed);
        return;
    }
    let mut taken: HashSet<String> = out.names(&dest).await.unwrap_or_default();
    let job = std::sync::Arc::new(job);
    for path in paths {
        if p.cancel.load(Ordering::Relaxed) {
            break;
        }
        let name = source::name(&path).to_string();
        *p.current.lock().unwrap() = name.clone();
        let r = match op.read(&path).await {
            Ok(bytes) => {
                let job = job.clone();
                tokio::task::spawn_blocking(move || convert(&bytes, &name, &job)).await.unwrap_or_else(|e| Err(("batch-failed", e.to_string())))
            }
            Err((id, detail)) => Err((id, detail)),
        };
        match r {
            Ok((new_name, data)) => {
                let new_name = source::free_name(&taken, &new_name);
                let to = format!("{dest}{new_name}");
                match crate::loader::write_whole(&out, &to, data).await {
                    Ok(()) => {
                        taken.insert(new_name);
                        p.saved.lock().unwrap().push(to);
                    }
                    Err(e) => p.failed.lock().unwrap().push(e),
                }
            }
            Err(e) => p.failed.lock().unwrap().push(e),
        }
        p.done.fetch_add(1, Ordering::Relaxed);
    }
    p.finished.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use image::DynamicImage;

    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(target(4000, 3000, Size::Same, false), (4000, 3000));
        assert_eq!(target(4000, 3000, Size::Percent(50), false), (2000, 1500));
        assert_eq!(target(4000, 3000, Size::Percent(200), false), (4000, 3000));
        assert_eq!(target(4000, 3000, Size::Percent(200), true), (8000, 6000));
        // Inside the box, the proportions kept: here the height decides.
        assert_eq!(target(4000, 3000, Size::Fit(1920, 1080), false), (1440, 1080));
        assert_eq!(target(3000, 4000, Size::Fit(1920, 1920), false), (1440, 1920));
        assert_eq!(target(800, 600, Size::Fit(1920, 1080), false), (800, 600));
        assert_eq!(target(800, 600, Size::Fit(1920, 1080), true), (1440, 1080));
        assert_eq!(target(5000, 10, Size::Percent(1), false), (50, 1));
        assert!(Job { format: None, ..Default::default() }.does_nothing());
        assert_eq!(with_ext("a.b.jpeg", Format::Png), "a.b.png");
        assert_eq!(with_ext("noext", Format::Webp), "noext.webp");
    }

    fn file(img: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut v = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut v), format).unwrap();
        v
    }

    #[test]
    fn convert_images() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(400, 300, |x, y| image::Rgb([x as u8, y as u8, 9])));
        let png = file(&img, image::ImageFormat::Png);
        // Another format and a smaller size.
        let job = Job { format: Some(Format::Webp), size: Size::Fit(200, 200), ..Default::default() };
        let (name, data) = convert(&png, "a.png", &job).unwrap();
        assert_eq!(name, "a.webp");
        let back = crate::decode::decode_full(&data, &name).unwrap();
        assert_eq!((back.width(), back.height()), (200, 150));
        // Its own format, half the size: still a PNG with the same name.
        let job = Job { format: None, size: Size::Percent(50), filter: Filter::Nearest, ..Default::default() };
        let (name, data) = convert(&png, "a.png", &job).unwrap();
        assert_eq!(name, "a.png");
        let back = crate::decode::decode_full(&data, &name).unwrap();
        assert_eq!((back.width(), back.height()), (200, 150));
        // Its own format cannot be written: left out, and said why.
        let tga = file(&img, image::ImageFormat::Tga);
        assert_eq!(convert(&tga, "a.tga", &job).unwrap_err(), (SKIP_FORMAT, "a.tga".to_string()));
        // An animation would lose its frames: left out, whatever the format.
        let apng = crate::encode::tests::apng(&png);
        assert_eq!(convert(&apng, "b.png", &Job::default()).unwrap_err(), (SKIP_INSIDE, "b.png".to_string()));
        // A phone JPEG with more after the picture (here: a motion photo video): converted.
        let jpeg = [file(&img, image::ImageFormat::Jpeg).as_slice(), b"\0\0\0\x18ftypmp42"].concat();
        let job = Job { format: None, size: Size::Percent(50), ..Default::default() };
        let (name, data) = convert(&jpeg, "p.jpg", &job).unwrap();
        assert_eq!(name, "p.jpg");
        assert!(crate::encode::can_turn_in_place(&data, &name));
        // Turned a quarter and fitted: the box applies to the turned image.
        let job = Job { format: Some(Format::Png), size: Size::Fit(150, 1000), filter: Filter::Nearest, turn: Turn::default().then(crate::transform::Op::Cw), ..Default::default() };
        let (_, data) = convert(&png, "a.png", &job).unwrap();
        let back = crate::decode::decode_full(&data, "a.png").unwrap();
        assert_eq!((back.width(), back.height()), (150, 200));
        // The top left corner of the original (0, 0) is now at the top right.
        let corner = back.to_rgb8().get_pixel(149, 0).0;
        assert!(corner[0] <= 2 && corner[1] <= 2, "{corner:?}");
        // Only turned, own format: a JPEG of 16-pixel multiples turned without encoding again.
        let job = Job { format: None, turn: Turn::default().then(crate::transform::Op::Half), ..Default::default() };
        assert!(!job.does_nothing());
        let jpeg = file(&img.crop_imm(0, 0, 320, 240), image::ImageFormat::Jpeg);
        let (_, data) = convert(&jpeg, "q.jpg", &job).unwrap();
        assert_eq!(Some(data), crate::transform::jpeg_lossless(&jpeg, job.turn));
        // Not an image.
        assert_eq!(convert(b"nothing", "c.jpg", &Job::default()).unwrap_err().0, "batch-failed");
    }
}
