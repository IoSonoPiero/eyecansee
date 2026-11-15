//! The details under the thumbnails (#139): which ones the Options ask for, what the grid learns
//! from the file itself (its upright size in pixels, the date it was taken), and the line written
//! under the name. The size on disk and the modification date come from the folder listing.

use serde::{Deserialize, Serialize};

use crate::i18n::I18n;

/// What goes under each thumbnail; at first all of it (Piero, 15/11).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct Details {
    pub name: bool,
    pub pixels: bool,
    pub size: bool,
    pub date: bool,
    /// Of videos and sounds, beside the date.
    pub duration: bool,
    /// Of CAD drawings, the number and revision of the title block (#144), before the size.
    pub drawing: bool,
}

impl Default for Details {
    fn default() -> Self {
        Details { name: true, pixels: true, size: true, date: true, duration: true, drawing: true }
    }
}

/// Below this side the thumbnails are small: only the name stays under them.
pub const MIN_SIDE: f32 = 128.0;

/// What a file says of itself, kept in the thumbnail cache with its thumbnail.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Info {
    /// Width and height as the picture is shown, after the EXIF orientation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixels: Option<[u32; 2]>,
    /// When it was taken (EXIF `DateTimeOriginal`, a video's `creation_time`), ISO as the metadata
    /// panel has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken: Option<String>,
    /// Of a video or a sound, as the player writes it ("1:24").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    /// Of a CAD drawing, its number and revision ("A-101 rev. C", `cad_meta::summary`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing: Option<String>,
}

impl Info {
    /// From the bytes of an image whose thumbnail, upright, is `thumb` (width, height): the size
    /// the file stores, turned as the thumbnail is, and the date taken. `decoded`: the size of the
    /// picture decoded whole for its thumbnail, for the formats whose header is not read.
    pub fn of(bytes: &[u8], name: &str, thumb: (u32, u32), decoded: Option<[u32; 2]>) -> Info {
        let pixels = crate::decode::header(bytes, name).map(|h| upright(h.size, thumb)).or(decoded);
        let taken = crate::meta::read(bytes).and_then(|m| m.fields.into_iter().find(|(_, l, _)| *l == "meta-taken").map(|(_, _, v)| v));
        Info { pixels, taken, duration: None, drawing: None }
    }

    /// From the metadata panel's reading of a video or a sound (`video_read::video_meta`): its
    /// size as it plays (turned), when it was recorded, how long it lasts.
    pub fn of_video(meta: &crate::meta::Meta) -> Info {
        let field = |label: &str| meta.fields.iter().find(|(_, l, _)| *l == label).map(|(_, _, v)| v.clone());
        let px = |label: &str| field(label).and_then(|v| v.trim_end_matches(" px").parse::<u32>().ok());
        Info { pixels: px("meta-width").zip(px("meta-height")).map(|(w, h)| [w, h]), taken: field("meta-taken"), duration: field("meta-duration"), drawing: None }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Option<Info> {
        serde_json::from_str(s).ok()
    }
}

/// `stored` swapped when the thumbnail is the other way round (portrait against landscape).
fn upright(stored: [u32; 2], (tw, th): (u32, u32)) -> [u32; 2] {
    let [w, h] = stored;
    if w != h && tw != th && (w > h) != (tw > th) { [h, w] } else { [w, h] }
}

/// Which date the line shows: the one the photo was taken, or the file's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DateKind {
    /// A photo's date taken.
    Taken,
    /// A video's or a sound's date recorded.
    Recorded,
    Modified,
}

/// The rows under a thumbnail, each in its place in every cell: the name, the dimensions and size
/// on one row ("1920 × 1080 · 350 KB"), the date on its own (with the time it is too long beside
/// them: Piero, 15/11).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rows {
    pub name: bool,
    pub measures: bool,
    pub date: bool,
}

impl Rows {
    /// Small thumbnails (`big` false), or nothing ticked: only the name. The duration goes on the
    /// date's row ("01/11/2026 14:43 · 1:24"): beside dimensions and size it was cut.
    pub fn of(d: &Details, big: bool) -> Rows {
        let (measures, date) = ((d.pixels || d.size || d.drawing) && big, (d.date || d.duration) && big);
        Rows { name: d.name || !(measures || date), measures, date }
    }

    pub fn count(&self) -> usize {
        usize::from(self.name) + usize::from(self.measures) + usize::from(self.date)
    }
}

/// The row of dimensions and size, the chosen ones joined by a dot; `None` with nothing to write
/// for this file. `pixels` and `size` already in words, `None` where they do not apply (no pixels
/// for a sound); a drawing's number and revision first.
pub fn measures(d: &Details, drawing: Option<String>, pixels: Option<String>, size: Option<String>) -> Option<String> {
    let parts: Vec<String> = [drawing.filter(|_| d.drawing), pixels.filter(|_| d.pixels), size.filter(|_| d.size)].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// "1920 × 1080".
pub fn pixels_text(i18n: &I18n, [w, h]: [u32; 2]) -> String {
    i18n.t_args("thumb-pixels", &[("w", &w.to_string()), ("h", &h.to_string())])
}

/// The tip over the date: which date it is.
pub fn date_tip(i18n: &I18n, kind: DateKind, date: &str) -> String {
    let id = match kind {
        DateKind::Taken => "thumb-taken-on",
        DateKind::Recorded => "thumb-recorded-on",
        DateKind::Modified => "thumb-modified-on",
    };
    i18n.t_args(id, &[("date", date)])
}

impl crate::App {
    /// The rows under the thumbnails.
    pub(crate) fn detail_rows(&self) -> Rows {
        Rows::of(&self.settings.thumb_details, self.thumb_size >= MIN_SIDE)
    }

    /// The details of the files `files` (indices among the images, those on screen) asked once each,
    /// when the line shows something read from the file.
    pub(crate) fn ask_infos(&mut self, files: std::ops::Range<usize>) {
        let rows = self.detail_rows();
        let d = self.settings.thumb_details;
        if !(rows.measures && (d.pixels || d.drawing) || rows.date && (d.date || d.duration)) {
            return;
        }
        let Some(loader) = &self.loader else { return };
        let Some(Ok(listing)) = self.listings.get(&self.dir) else { return };
        let wanted: Vec<_> = listing.files.get(files).unwrap_or_default().iter().filter(|f| !self.thumb_infos.contains_key(&f.path) && !self.info_asked.contains(&f.path)).cloned().collect();
        if wanted.is_empty() {
            return;
        }
        self.info_asked.extend(wanted.iter().map(|f| f.path.clone()));
        loader.load_infos(self.generation, wanted);
    }

    /// The row of dimensions and size of `file`, and the row of its date and duration, with the tip
    /// over the date saying which date it is. What is known is shown at once, the rest as it
    /// comes: the modification date until the date taken is read.
    pub(crate) fn details_of(&self, file: &crate::source::File) -> (Option<String>, Option<(String, Option<String>)>) {
        let d = self.settings.thumb_details;
        let info = self.thumb_infos.get(&file.path);
        let pixels = info.and_then(|i| i.pixels).map(|p| pixels_text(&self.i18n, p));
        let drawing = info.and_then(|i| i.drawing.clone());
        let measures = measures(&d, drawing, pixels, Some(self.human_size(file.size)));
        let date = info
            .and_then(|i| i.taken.as_deref())
            .map(|t| (self.meta_value("meta-taken", t), if crate::decode::is_played(&file.path) { DateKind::Recorded } else { DateKind::Taken }))
            .or_else(|| self.file_date(&file.modified).map(|m| (m, DateKind::Modified)))
            .filter(|_| d.date);
        let duration = info.and_then(|i| i.duration.clone()).filter(|_| d.duration);
        let tip = date.as_ref().map(|(t, kind)| date_tip(&self.i18n, *kind, t));
        let parts: Vec<String> = [date.map(|(t, _)| t), duration].into_iter().flatten().collect();
        (measures, (!parts.is_empty()).then(|| (parts.join(" · "), tip)))
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows() {
        let all = Details { name: true, pixels: true, size: true, date: true, duration: true, drawing: true };
        let (px, kb) = (Some("1920 × 1080".to_string()), Some("350 KB".to_string()));
        assert_eq!(measures(&all, None, px.clone(), kb.clone()).as_deref(), Some("1920 × 1080 · 350 KB"));
        // Only what is chosen; a sound has no pixels: the size stays.
        assert_eq!(measures(&Details { pixels: false, ..all }, None, px.clone(), kb.clone()).as_deref(), Some("350 KB"));
        assert_eq!(measures(&all, None, None, kb.clone()).as_deref(), Some("350 KB"));
        // A drawing's number and revision before its size, if chosen.
        let number = Some("A-101 rev. C".to_string());
        assert_eq!(measures(&all, number.clone(), None, kb.clone()).as_deref(), Some("A-101 rev. C · 350 KB"));
        assert_eq!(measures(&Details { drawing: false, ..all }, number.clone(), None, kb.clone()).as_deref(), Some("350 KB"));
        let name_only = Details { name: true, pixels: false, size: false, date: false, duration: false, drawing: false };
        assert_eq!(measures(&name_only, number, px, kb), None);
        // The rows: name, measures, date; small thumbnails or nothing ticked, the name alone.
        assert_eq!(Rows::of(&all, true), Rows { name: true, measures: true, date: true });
        assert_eq!(Rows::of(&all, true).count(), 3);
        assert_eq!(Rows::of(&all, false), Rows { name: true, measures: false, date: false });
        assert_eq!(Rows::of(&Details { name: false, ..name_only }, true), Rows { name: true, measures: false, date: false });
        assert_eq!(Rows::of(&Details { name: false, date: true, ..name_only }, true).count(), 1);
        // The duration alone takes the date's row; the drawing's number alone the measures' row.
        assert_eq!(Rows::of(&Details { duration: true, ..name_only }, true), Rows { name: true, measures: false, date: true });
        assert_eq!(Rows::of(&Details { drawing: true, ..name_only }, true), Rows { name: true, measures: true, date: false });
    }

    /// A format whose header is not read (an SGI, a PCX…): the size of the picture decoded for
    /// its thumbnail.
    #[test]
    fn size_of_a_rare_format() {
        let info = Info::of(b"not a header anyone reads", "x.rgb", (256, 192), Some([800, 600]));
        assert_eq!(info.pixels, Some([800, 600]));
        assert_eq!(Info::of(b"", "x.rgb", (256, 192), None).pixels, None);
    }

    #[test]
    fn info_of_a_video() {
        use crate::meta::{Group, Meta};
        let meta = Meta {
            fields: vec![
                (Group::Shot, "meta-taken", "2026-10-09 17:48:35".into()),
                (Group::Shot, "meta-duration", "1:24".into()),
                (Group::Image, "meta-width", "1080 px".into()),
                (Group::Image, "meta-height", "1920 px".into()),
            ],
            gps: None,
        };
        assert_eq!(Info::of_video(&meta), Info { pixels: Some([1080, 1920]), taken: Some("2026-10-09 17:48:35".into()), duration: Some("1:24".into()), drawing: None });
        // A sound: no pixels.
        let sound = Meta { fields: vec![(Group::Shot, "meta-duration", "3:00".into())], gps: None };
        assert_eq!(Info::of_video(&sound), Info { duration: Some("3:00".into()), ..Info::default() });
    }

    #[test]
    fn upright_as_the_thumbnail() {
        // A phone photo stored 4000 × 3000 with "turn by a quarter": its thumbnail stands.
        assert_eq!(upright([4000, 3000], (192, 256)), [3000, 4000]);
        assert_eq!(upright([4000, 3000], (256, 192)), [4000, 3000]);
        // Square ones never swap.
        assert_eq!(upright([1000, 1000], (100, 200)), [1000, 1000]);
        assert_eq!(upright([4000, 3000], (256, 256)), [4000, 3000]);
    }

    #[test]
    fn info_of_a_jpeg_and_back() {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(64, 32).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();
        let info = Info::of(&bytes, "a.jpg", (64, 32), None);
        assert_eq!(info, Info { pixels: Some([64, 32]), taken: None, duration: None, drawing: None });
        assert_eq!(Info::from_json(&info.to_json()), Some(info));
        // A cache row of another version (fields more or fewer) still reads.
        assert_eq!(Info::from_json(r#"{"pixels":[3,2],"later":1}"#).unwrap().pixels, Some([3, 2]));
        assert_eq!(Info::from_json("{}"), Some(Info::default()));
    }

    #[test]
    fn options_of_an_older_version() {
        let d: Details = serde_json::from_str("{}").unwrap();
        assert_eq!(d, Details::default());
        // A choice saved before: what is there stays, what is missing is on, as at first.
        let d: Details = serde_json::from_str(r#"{"size":false}"#).unwrap();
        assert!(d.name && !d.size && d.pixels && d.date && d.duration);
    }

    #[test]
    fn texts_in_each_language() {
        for lang in ["en", "it"] {
            let i18n = I18n::new(lang);
            assert_eq!(pixels_text(&i18n, [1920, 1080]), "1920 × 1080");
            for kind in [DateKind::Taken, DateKind::Recorded, DateKind::Modified] {
                let tip = date_tip(&i18n, kind, "14/11/2026");
                assert!(tip.contains("14/11/2026") && !tip.contains("thumb-"), "{lang}: {tip}");
            }
        }
    }
}
