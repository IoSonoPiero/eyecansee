//! Save as… on a video in the viewer (#16): the frame shown, paused, at the video's own size,
//! saved with the options of Save as…. The image keeps what the video says of it (the rule of the
//! metadata): the camera, the moment of the frame (when the recording started plus the frame's
//! time) and the place (the dashcam's track at that moment, the file's own location, or the
//! drone's starting point), as EXIF in the formats that hold it.

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use exif::{Field, In, Rational, Tag, Value};
use image::{DynamicImage, RgbaImage};

use crate::App;
use crate::source::Store;

/// Latitude, longitude (decimal degrees, negative to the south and west), altitude in metres.
type Place = (f64, f64, Option<f64>);

/// What the frame keeps of its video.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct FrameMeta {
    make: Option<String>,
    model: Option<String>,
    /// The moment the frame shows, in the computer's time zone (EXIF dates are local).
    taken: Option<jiff::Zoned>,
    place: Option<Place>,
}

/// The name proposed for the frame at `at` seconds of the video `name`, as VLC names its
/// snapshots: "clip 1m02s345.jpg" with `format`'s extension. No ":" (not allowed on Windows).
pub(crate) fn frame_name(name: &str, at: f64, format: crate::encode::Format) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let ms = (at.max(0.0) * 1000.0).round() as u64;
    let (h, m, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    let time = if h > 0 { format!("{h}h{m:02}m{s:02}s{ms:03}") } else { format!("{m}m{s:02}s{ms:03}") };
    format!("{stem} {time}.{}", format.ext())
}

/// The frame's metadata from the video's `tags` (`mpv::info`), `at` seconds from its start.
/// `track`: the dashcam's point at that moment (`gps_track`); `srt`: the drone's starting point.
pub(crate) fn frame_meta(tags: &[(&str, String)], at: f64, track: Option<Place>, srt: Option<(f64, f64)>, tz: &jiff::tz::TimeZone) -> FrameMeta {
    let tag = |keys: &[&str]| keys.iter().find_map(|k| tags.iter().find(|(t, _)| t == k).map(|(_, v)| v.trim().to_string())).filter(|v| !v.is_empty());
    let taken = tag(&["creation_time", "date_recorded"])
        .and_then(|t| crate::meta::first(&t).parse::<jiff::Timestamp>().ok())
        .filter(|t| t.as_second() > 0)
        .and_then(|t| t.checked_add(jiff::SignedDuration::from_secs_f64(at.max(0.0))).ok())
        .map(|t| t.to_zoned(tz.clone()));
    let located = tag(&["location", "com.apple.quicktime.location.ISO6709"]).and_then(|l| crate::meta::iso6709(&l));
    FrameMeta {
        make: tag(&["com.android.manufacturer", "com.apple.quicktime.make", "make"]),
        model: tag(&["com.android.model", "com.apple.quicktime.model", "model"]),
        taken,
        place: track.or(located).or(srt.map(|(lat, lon)| (lat, lon, None))),
    }
}

/// `meta` as an EXIF block (TIFF data, as `encode::Original` keeps it); `None` with nothing to say.
pub(crate) fn exif(meta: &FrameMeta) -> Option<Vec<u8>> {
    let field = |tag, value| Field { tag, ifd_num: In::PRIMARY, value };
    let ascii = |s: &str| Value::Ascii(vec![s.as_bytes().to_vec()]);
    let mut fields = Vec::new();
    for (tag, value) in [(Tag::Make, &meta.make), (Tag::Model, &meta.model)] {
        if let Some(v) = value {
            fields.push(field(tag, ascii(v)));
        }
    }
    if let Some(t) = &meta.taken {
        let date = t.strftime("%Y:%m:%d %H:%M:%S").to_string();
        let offset = t.strftime("%:z").to_string();
        let sub = format!("{:03}", t.millisecond());
        fields.extend([field(Tag::DateTimeOriginal, ascii(&date)), field(Tag::DateTimeDigitized, ascii(&date))]);
        fields.extend([field(Tag::OffsetTimeOriginal, ascii(&offset)), field(Tag::OffsetTimeDigitized, ascii(&offset))]);
        fields.extend([field(Tag::SubSecTimeOriginal, ascii(&sub)), field(Tag::SubSecTimeDigitized, ascii(&sub))]);
    }
    if let Some((lat, lon, alt)) = meta.place {
        fields.push(field(Tag::GPSVersionID, Value::Byte(vec![2, 3, 0, 0])));
        fields.push(field(Tag::GPSLatitudeRef, ascii(if lat < 0.0 { "S" } else { "N" })));
        fields.push(field(Tag::GPSLatitude, dms(lat)));
        fields.push(field(Tag::GPSLongitudeRef, ascii(if lon < 0.0 { "W" } else { "E" })));
        fields.push(field(Tag::GPSLongitude, dms(lon)));
        if let Some(alt) = alt {
            fields.push(field(Tag::GPSAltitudeRef, Value::Byte(vec![u8::from(alt < 0.0)])));
            fields.push(field(Tag::GPSAltitude, Value::Rational(vec![Rational::from(((alt.abs() * 100.0).round() as u32, 100))])));
        }
    }
    crate::meta::write_exif(&fields, false)
}

/// Degrees as EXIF writes them: degrees, minutes, seconds (to a ten-thousandth).
fn dms(v: f64) -> Value {
    let a = v.abs();
    let (d, rest) = (a.trunc(), a.fract() * 60.0);
    let (m, s) = (rest.trunc(), rest.fract() * 60.0);
    Value::Rational(vec![Rational::from((d as u32, 1)), Rational::from((m as u32, 1)), Rational::from(((s * 10_000.0).round() as u32, 10_000))])
}

/// The frame `image` (sent by the player) with the metadata of the video `path`, as Save as…
/// takes an original: the image, its preview and the video's size.
pub(crate) async fn original(op: &Store, path: &str, at: f64, image: Receiver<RgbaImage>) -> Result<(Arc<crate::encode::Original>, eframe::egui::ColorImage, u64), String> {
    let size = op.size(path).await.map_err(|(_, detail)| detail)?;
    let input = crate::video_read::video_input(tokio::runtime::Handle::current(), op, path, size);
    let file = op.local_file(path);
    let srt = crate::video_read::drone_start(op, path).await;
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let image = image.recv_timeout(Duration::from_secs(20)).map_err(|_| "no frame from the video".to_string())?;
        // Without its metadata (a file mpv cannot read again) the frame is saved all the same.
        let info = crate::mpv::info(input).unwrap_or_default();
        let start = crate::meta::video_start(&info);
        let track = file.zip(start).and_then(|(f, s)| {
            let t = s + at.max(0.0).round() as i64;
            crate::gps_track::points(&f, t - 2, t + 2).into_iter().min_by_key(|p| (p.0 - t).abs())
        });
        let track = track.map(|(_, lat, lon, alt)| (lat, lon, Some(alt)));
        let meta = frame_meta(&info.tags, at, track, srt, &jiff::tz::TimeZone::system());
        log!("frame at {at:.3} s of {path:?}: {}×{}, {meta:?}", image.width(), image.height());
        // Opaque: no note about transparency in Save as….
        let image = DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(image).into_rgb8());
        let preview = crate::decode::preview(&image);
        Ok((Arc::new(crate::encode::Original::with_exif(image, exif(&meta))), preview, size))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

impl App {
    /// Save as… on the video in the viewer: paused, its frame asked to the player, with the time
    /// it shows; `None` when `path` is not the video playing.
    pub(crate) fn convert_frame(&mut self, path: &str, asked: u64) -> Option<f64> {
        let player = self.player_of(path)?;
        player.pause();
        let at = player.position();
        let (tx, rx) = std::sync::mpsc::channel();
        // The player gives the frame as the file has it: the turn not saved yet is added here.
        let turn = self.turn.turns_only();
        player.grab(move |image| {
            let mut image = image::DynamicImage::ImageRgba8(image);
            turn.apply(&mut image);
            let _ = tx.send(image.into_rgba8());
        });
        self.loader.as_ref()?.frame_original(asked, path.to_string(), at, rx);
        Some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minutes, seconds and thousandths, hours when there are; never ":" nor ".".
    #[test]
    fn frame_names() {
        use crate::encode::Format;
        assert_eq!(frame_name("clip.mp4", 12.3454, Format::Jpeg), "clip 0m12s345.jpg");
        assert_eq!(frame_name("DJI_0001.MP4", 3725.0, Format::Jpeg), "DJI_0001 1h02m05s000.jpg");
        assert_eq!(frame_name("no extension", 0.0, Format::Png), "no extension 0m00s000.png");
        // Another format chosen afterwards changes only the extension, also after a dot in the name.
        assert_eq!(crate::batch::with_ext(&frame_name("a.b.mov", 61.5, Format::Jpeg), Format::Png), "a.b 1m01s500.png");
    }

    /// The date moves with the frame and is written in local time with its offset; the dashcam's
    /// point wins over the file's location, which wins over the drone's start.
    #[test]
    fn meta_of_the_frame() {
        let tz = jiff::tz::TimeZone::fixed(jiff::tz::offset(2));
        let tags = vec![("creation_time", "2026-06-14T08:30:00.000000Z".to_string()), ("make", "Acme".into()), ("model", "Cam 9".into()), ("location", "+45.4641+009.1919/".into())];
        let m = frame_meta(&tags, 12.5, None, Some((1.0, 2.0)), &tz);
        assert_eq!((m.make.as_deref(), m.model.as_deref()), (Some("Acme"), Some("Cam 9")));
        assert_eq!(m.taken.as_ref().map(|t| t.strftime("%Y-%m-%d %H:%M:%S%.3f %:z").to_string()).as_deref(), Some("2026-06-14 10:30:12.500 +02:00"));
        assert_eq!(m.place, Some((45.4641, 9.1919, None)));
        assert_eq!(frame_meta(&tags, 0.0, Some((40.0, 18.0, Some(12.0))), None, &tz).place, Some((40.0, 18.0, Some(12.0))));
        assert_eq!(frame_meta(&[], 0.0, None, Some((1.0, 2.0)), &tz).place, Some((1.0, 2.0, None)));
        assert_eq!(frame_meta(&[], 5.0, None, None, &tz), FrameMeta::default());
    }

    /// Read back by the metadata panel: camera, date, place and altitude, south and west negative.
    #[test]
    fn exif_read_back() {
        let tz = jiff::tz::TimeZone::fixed(jiff::tz::offset(2));
        let tags = vec![("creation_time", "2026-06-14T08:30:00Z".to_string()), ("com.android.manufacturer", "Xiaomi".into()), ("com.android.model", "POCO X7 Pro".into())];
        let m = frame_meta(&tags, 1.25, Some((-33.8568, -151.2153, Some(-4.5))), None, &tz);
        let read = crate::meta::read(&exif(&m).unwrap()).unwrap();
        let get = |id| read.fields.iter().find(|(_, l, _)| *l == id).map(|(_, _, v)| v.clone());
        assert_eq!(get("meta-make").as_deref(), Some("Xiaomi"));
        assert_eq!(get("meta-model").as_deref(), Some("POCO X7 Pro"));
        assert!(get("meta-taken").is_some_and(|t| t.starts_with("2026-06-14 10:30:01")), "{:?}", get("meta-taken"));
        let (lat, lon) = read.gps.unwrap();
        assert!((lat + 33.8568).abs() < 1e-5 && (lon + 151.2153).abs() < 1e-5, "{lat} {lon}");
        assert!(exif(&FrameMeta::default()).is_none());
    }

    /// With libmpv and a real video (`EYECANSEE_FRAME_VIDEO`): paused at 2.5 s and drawn small,
    /// as in the viewer, the frame comes at the video's own size, and saved as JPEG
    /// (`EYECANSEE_FRAME_OUT`) with its camera, date and place for exiftool to check.
    #[test]
    #[ignore]
    fn frame_live() {
        use std::time::Instant;
        let video = std::path::PathBuf::from(std::env::var("EYECANSEE_FRAME_VIDEO").expect("EYECANSEE_FRAME_VIDEO"));
        let out = std::env::var("EYECANSEE_FRAME_OUT").expect("EYECANSEE_FRAME_OUT");
        let player = crate::mpv::Player::open(crate::mpv::Input::File(video.clone()), 0, true, || {}).unwrap();
        let start = Instant::now();
        while player.video_size().is_none() {
            assert!(start.elapsed() < Duration::from_secs(20), "the video does not open");
            std::thread::sleep(Duration::from_millis(50));
        }
        player.pause();
        player.seek(2.5, true);
        std::thread::sleep(Duration::from_secs(1));
        player.set_size([320, 180]);
        let (tx, rx) = std::sync::mpsc::channel();
        let asked = Instant::now();
        player.grab(move |image| {
            let _ = tx.send(image);
        });
        let image = rx.recv_timeout(Duration::from_secs(10)).expect("a frame");
        eprintln!("frame {}×{} in {:.0} ms at {:.3} s", image.width(), image.height(), asked.elapsed().as_secs_f64() * 1000.0, player.position());
        assert_eq!(Some([image.width(), image.height()]), player.video_size());
        let info = crate::mpv::info(crate::mpv::Input::File(video)).unwrap();
        let meta = frame_meta(&info.tags, player.position(), None, None, &jiff::tz::TimeZone::system());
        eprintln!("{meta:?}");
        let original = crate::encode::Original::with_exif(DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(image).into_rgb8()), exif(&meta));
        std::fs::write(out, original.encode(&crate::encode::Options::default()).unwrap()).unwrap();
    }
}
