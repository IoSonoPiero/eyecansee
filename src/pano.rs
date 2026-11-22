//! 360° panoramas (#188): recognised from the Google Photo Sphere tags in their XMP (`GPano`), and
//! the sums of the view from inside the sphere: which pixel of the picture each direction shows,
//! where each direction falls on the screen, and the grid of small triangles the viewer draws
//! the picture on (`pano_view`). No egui here.
//!
//! Directions: `lon` (longitude) from the middle of the whole panorama, positive to the right,
//! `lat` (latitude) positive up; x to the right, y up, z ahead. The picture covers the part of the
//! sphere its cropped area says: a picture without the cropped area is the whole sphere.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// The namespace of the Google Photo Sphere tags.
pub(crate) const GPANO: &str = "http://ns.google.com/photos/1.0/panorama/";

/// The field of view across the longer side of the view: the widest and the narrowest.
pub(crate) const WIDEST: f32 = 120.0 * PI / 180.0;
pub(crate) const NARROWEST: f32 = 10.0 * PI / 180.0;
/// The field of view of the first look, when the file does not give one.
const FIRST_FOV: f32 = 90.0 * PI / 180.0;
/// The grid the picture is drawn on: a vertex every 2° of the sphere at most. Smaller triangles
/// follow the sphere more closely (a triangle is drawn flat), more cost more to project; at 2° a
/// point is off by at most a pixel or so at 90° (`the_triangles_show_what_the_sums_say`).
const STEP: f32 = 2.0 * PI / 180.0;
/// Closest to the side of the eye a vertex may be (cosine of the angle from straight ahead):
/// triangles with a vertex further aside are not drawn. With at most `WIDEST` across the longer
/// side, what is on the screen is within 68° of straight ahead, cosine 0.37.
const NEAR: f32 = 0.17;

/// The part of the sphere a picture covers, in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pano {
    /// Longitude of the picture's left edge, and how far it goes to the right.
    pub(crate) lon0: f32,
    pub(crate) lon_span: f32,
    /// Latitude of its top edge, and how far it goes down.
    pub(crate) lat_top: f32,
    pub(crate) lat_span: f32,
    /// Width over height of the cropped area, in pixels: what the picture's own shape must be.
    pub(crate) aspect: f32,
    /// The first look, from the file (`InitialView…`) or the middle of the picture.
    pub(crate) first: View,
}

/// Where the eye looks: `yaw` and `pitch` of the middle of the view, `fov` across its longer side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct View {
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) fov: f32,
}

/// The panorama a picture declares in its XMP, if any.
pub(crate) fn detect(bytes: &[u8]) -> Option<Pano> {
    let xmp = xmp_of(bytes)?;
    from_xmp(std::str::from_utf8(&xmp).ok()?)
}

/// The XMP packet of a file: a JPEG's from its segments (the extended part too), any other's
/// (PNG, TIFF, WebP, HEIC…) found in its first megabytes.
fn xmp_of(bytes: &[u8]) -> Option<std::borrow::Cow<'_, [u8]>> {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return crate::meta_write::jpeg_xmp_whole(bytes).map(std::borrow::Cow::Owned);
    }
    let head = &bytes[..bytes.len().min(16 << 20)];
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    let start = find(head, b"<x:xmpmeta")?;
    let end = start + find(&head[start..], b"</x:xmpmeta>")? + b"</x:xmpmeta>".len();
    Some(std::borrow::Cow::Borrowed(&head[start..end]))
}

/// From the `GPano` properties, as attributes or as elements: `ProjectionType` equirectangular and
/// `UsePanoramaViewer` not False; the sizes of the whole panorama and of the cropped area, when
/// given, must make sense, or the picture stays flat.
pub(crate) fn from_xmp(xmp: &str) -> Option<Pano> {
    let props = props(xmp)?;
    let text = |k: &str| props.get(k).map(String::as_str);
    let number = |k: &str| text(k).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite());
    // A number given that is not one: the file is not what it says, the picture stays flat.
    if props.iter().any(|(k, v)| (k.ends_with("Pixels") || k.ends_with("Degrees")) && v.parse::<f32>().ok().is_none_or(|n| !n.is_finite())) {
        return None;
    }
    if !text("ProjectionType")?.eq_ignore_ascii_case("equirectangular") || text("UsePanoramaViewer").is_some_and(|v| v.eq_ignore_ascii_case("false")) {
        return None;
    }
    let full_w = number("FullPanoWidthPixels");
    let full_h = number("FullPanoHeightPixels").or(full_w.map(|w| w / 2.0));
    let (full_w, full_h) = match (full_w, full_h) {
        (Some(w), Some(h)) => (w, h),
        _ => (1.0, 0.5),
    };
    let crop_w = number("CroppedAreaImageWidthPixels").unwrap_or(full_w);
    let crop_h = number("CroppedAreaImageHeightPixels").unwrap_or(full_h);
    let (left, top) = (number("CroppedAreaLeftPixels").unwrap_or(0.0), number("CroppedAreaTopPixels").unwrap_or(0.0));
    let room = 1.0 + 1e-3;
    let sane = full_w > 0.0 && full_h > 0.0 && crop_w > 0.0 && crop_h > 0.0 && left >= 0.0 && top >= 0.0 && left < full_w && crop_w <= full_w * room && top + crop_h <= full_h * room;
    if !sane {
        return None;
    }
    let lon_span = (crop_w / full_w * TAU).min(TAU);
    let lat_span = (crop_h / full_h * PI).min(PI);
    let first = View { yaw: 0.0, pitch: 0.0, fov: FIRST_FOV };
    let mut pano = Pano { lon0: left / full_w * TAU - PI, lon_span, lat_top: FRAC_PI_2 - top / full_h * PI, lat_span, aspect: crop_w / crop_h, first };
    let degrees = |k: &str| number(k).map(f32::to_radians);
    // The heading of the first look is a compass heading, as `PoseHeadingDegrees` is of the middle.
    let yaw = degrees("InitialViewHeadingDegrees").map(|h| h - degrees("PoseHeadingDegrees").unwrap_or(0.0));
    pano.first = View {
        yaw: yaw.map_or(pano.lon0 + lon_span / 2.0, wrap),
        pitch: degrees("InitialViewPitchDegrees").unwrap_or(pano.lat_top - lat_span / 2.0),
        fov: degrees("InitialHorizontalFOVDegrees").unwrap_or(FIRST_FOV),
    };
    Some(pano)
}

/// The `GPano` properties of an XMP packet, as attributes or as elements, by their names; `None`
/// without any, or when the packet is not well-formed.
pub(crate) fn props(xmp: &str) -> Option<std::collections::HashMap<String, String>> {
    if !xmp.contains(GPANO) {
        return None;
    }
    let doc = roxmltree::Document::parse(xmp).ok()?;
    let mut props = std::collections::HashMap::new();
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        if node.tag_name().namespace() == Some(GPANO)
            && let Some(text) = node.text()
        {
            props.insert(node.tag_name().name().to_string(), text.trim().to_string());
        }
        for a in node.attributes().filter(|a| a.namespace() == Some(GPANO)) {
            props.insert(a.name().to_string(), a.value().trim().to_string());
        }
    }
    (!props.is_empty()).then_some(props)
}

/// An angle in (-π, π].
fn wrap(a: f32) -> f32 {
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI { w + TAU } else { w }
}

pub(crate) fn direction(lon: f32, lat: f32) -> [f32; 3] {
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}

/// The way back of `direction`, for the tests' sums (the viewer goes the other way: `Pano::at`).
#[cfg(test)]
fn lon_lat(d: [f32; 3]) -> (f32, f32) {
    (d[0].atan2(d[2]), d[1].clamp(-1.0, 1.0).asin())
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl Pano {
    /// A picture `size` pixels large has the shape of the cropped area, within 2%: a picture
    /// resized keeps it, one turned on its side or cropped after the tags were written does not.
    pub(crate) fn fits(&self, size: [f32; 2]) -> bool {
        size[1] > 0.0 && (size[0] / size[1] / self.aspect - 1.0).abs() < 0.02
    }

    /// The whole way round.
    pub(crate) fn round(&self) -> bool {
        self.lon_span >= TAU - 1e-3
    }

    /// Where direction `d` is on the picture, from 0 to 1 on each side; `None` outside it. The
    /// tests check the grid against it.
    #[cfg(test)]
    pub(crate) fn uv(&self, d: [f32; 3]) -> Option<[f32; 2]> {
        let (lon, lat) = lon_lat(d);
        let mut along = (lon - self.lon0).rem_euclid(TAU);
        // The right edge of a round panorama is its left edge too.
        if self.round() && along < 1e-6 && d[2] < 0.0 {
            along = TAU;
        }
        let u = along / self.lon_span;
        let v = (self.lat_top - lat) / self.lat_span;
        let edge = 1e-4;
        ((-edge..=1.0 + edge).contains(&u) && (-edge..=1.0 + edge).contains(&v)).then_some([u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)])
    }

    /// The pixel of a picture `size` pixels large that direction `d` shows.
    #[cfg(test)]
    pub(crate) fn pixel(&self, d: [f32; 3], size: [f32; 2]) -> Option<[f32; 2]> {
        self.uv(d).map(|[u, v]| [u * size[0], v * size[1]])
    }

    /// The direction of the point `[u, v]` of the picture.
    pub(crate) fn at(&self, [u, v]: [f32; 2]) -> [f32; 3] {
        direction(self.lon0 + u * self.lon_span, self.lat_top - v * self.lat_span)
    }

    /// `view` brought inside what can be looked at on a screen `size` pixels large: the field of
    /// view between `NARROWEST` and `WIDEST` (and not wider than a partial panorama), the yaw
    /// round and round on a round panorama, else kept so the view stays on the picture, as the
    /// pitch, which goes up to the poles of a whole sphere.
    pub(crate) fn clamped(&self, view: View, size: [f32; 2]) -> View {
        let widest = if self.round() { WIDEST } else { WIDEST.min(self.lon_span.max(self.lat_span)).max(NARROWEST) };
        let fov = view.fov.clamp(NARROWEST, widest);
        let v = View { fov, ..view };
        let [half_w, half_h] = [v.across(size, 0) / 2.0, v.across(size, 1) / 2.0];
        let inside = |c: f32, low: f32, high: f32, half: f32| if high - low <= 2.0 * half { (low + high) / 2.0 } else { c.clamp(low + half, high - half) };
        let yaw = if self.round() { wrap(view.yaw) } else { inside(view.yaw, self.lon0, self.lon0 + self.lon_span, half_w) };
        let (bottom, top) = (self.lat_top - self.lat_span, self.lat_top);
        let pitch = if self.lat_span >= PI - 1e-3 { view.pitch.clamp(-FRAC_PI_2, FRAC_PI_2) } else { inside(view.pitch, bottom, top, half_h) };
        View { yaw, pitch, fov }
    }

    /// Screen pixels for each pixel of a picture `width` pixels wide, in the middle of the view:
    /// the zoom of the viewer's status bar (`zoom::of_file` of it).
    pub(crate) fn scale(&self, view: View, size: [f32; 2], width: f32) -> f32 {
        view.focal(size) * self.lon_span / width
    }

    /// The field of view that shows the middle at `scale` (the inverse of `scale`).
    pub(crate) fn fov_for(&self, scale: f32, size: [f32; 2], width: f32) -> f32 {
        2.0 * (size[0].max(size[1]) / 2.0 / (scale * width / self.lon_span)).atan()
    }
}

impl View {
    /// Pixels from the eye to the screen: half the longer side over the tangent of half the fov.
    pub(crate) fn focal(&self, size: [f32; 2]) -> f32 {
        size[0].max(size[1]) / 2.0 / (self.fov / 2.0).tan()
    }

    /// The field of view across side `axis` (0 across, 1 up and down) of the screen.
    pub(crate) fn across(&self, size: [f32; 2], axis: usize) -> f32 {
        2.0 * (size[axis] / 2.0 / self.focal(size)).atan()
    }

    /// Right, up and ahead of the eye.
    fn axes(&self) -> [[f32; 3]; 3] {
        let (sy, cy, sp, cp) = (self.yaw.sin(), self.yaw.cos(), self.pitch.sin(), self.pitch.cos());
        [[cy, 0.0, -sy], [-sp * sy, cp, -sp * cy], [cp * sy, sp, cp * cy]]
    }

    /// Where direction `d` falls on a screen `size` pixels large (0,0 at its top left corner);
    /// `None` when it is too far aside to be drawn (behind the eye, or almost).
    pub(crate) fn project(&self, d: [f32; 3], size: [f32; 2]) -> Option<[f32; 2]> {
        let [r, u, f] = self.axes();
        let z = dot(d, f);
        if z < NEAR {
            return None;
        }
        let focal = self.focal(size);
        Some([size[0] / 2.0 + focal * dot(d, r) / z, size[1] / 2.0 - focal * dot(d, u) / z])
    }

    /// The direction seen at point `p` of a screen `size` pixels large (the way back of `project`).
    #[cfg(test)]
    pub(crate) fn ray(&self, p: [f32; 2], size: [f32; 2]) -> [f32; 3] {
        let [r, u, f] = self.axes();
        let focal = self.focal(size);
        let (x, y) = ((p[0] - size[0] / 2.0) / focal, -(p[1] - size[1] / 2.0) / focal);
        let d = [f[0] + x * r[0] + y * u[0], f[1] + x * r[1] + y * u[1], f[2] + x * r[2] + y * u[2]];
        let n = dot(d, d).sqrt();
        [d[0] / n, d[1] / n, d[2] / n]
    }

    /// Turned by a drag of `delta` screen pixels: the picture follows the pointer.
    pub(crate) fn dragged(&self, delta: [f32; 2], size: [f32; 2]) -> View {
        let per_pixel = 1.0 / self.focal(size);
        View { yaw: self.yaw - (delta[0] * per_pixel).atan(), pitch: self.pitch + (delta[1] * per_pixel).atan(), fov: self.fov }
    }

    /// Closer (`factor` > 1) or further: the field of view as a zoom by `factor` makes it.
    pub(crate) fn zoomed(&self, factor: f32) -> View {
        View { fov: 2.0 * ((self.fov / 2.0).tan() / factor).atan(), ..*self }
    }
}

/// The picture on a grid of vertices, `cols` × `rows`, row by row: each with its point on the
/// picture (0 to 1) and, when it can be drawn, its place on the screen.
pub(crate) struct Grid {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) uv: Vec<[f32; 2]>,
    pub(crate) screen: Vec<Option<[f32; 2]>>,
}

impl Grid {
    /// The triangles with all three vertices on the screen, as indices of the vertices.
    pub(crate) fn triangles(&self) -> impl Iterator<Item = [usize; 3]> + '_ {
        let quads = (0..self.rows - 1).flat_map(move |j| (0..self.cols - 1).map(move |i| j * self.cols + i));
        quads.flat_map(move |a| [[a, a + 1, a + self.cols + 1], [a, a + self.cols + 1, a + self.cols]]).filter(|t| t.iter().all(|&k| self.screen[k].is_some()))
    }
}

/// The grid of `pano` seen with `view` on a screen `size` pixels large.
pub(crate) fn grid(pano: &Pano, view: View, size: [f32; 2]) -> Grid {
    let cols = (pano.lon_span / STEP).ceil().max(1.0) as usize + 1;
    let rows = (pano.lat_span / STEP).ceil().max(1.0) as usize + 1;
    let mut uv = Vec::with_capacity(cols * rows);
    for j in 0..rows {
        for i in 0..cols {
            uv.push([i as f32 / (cols - 1) as f32, j as f32 / (rows - 1) as f32]);
        }
    }
    let screen = uv.iter().map(|&p| view.project(pano.at(p), size)).collect();
    Grid { cols, rows, uv, screen }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deg(d: f32) -> f32 {
        d.to_radians()
    }

    fn xmp(props: &str) -> String {
        format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GPano="{GPANO}" {props}/></rdf:RDF></x:xmpmeta>"#
        )
    }

    fn whole() -> Pano {
        from_xmp(&xmp(r#"GPano:ProjectionType="equirectangular" GPano:UsePanoramaViewer="True""#)).unwrap()
    }

    fn close(a: [f32; 2], b: [f32; 2], within: f32) -> bool {
        (a[0] - b[0]).abs() <= within && (a[1] - b[1]).abs() <= within
    }

    /// #188: the panorama of a DJI drone (attributes, the whole sphere), the same as elements,
    /// `UsePanoramaViewer` False, no GPano, another projection and sizes that make no sense.
    #[test]
    fn recognised_from_the_xmp() {
        let dji = xmp(r#"GPano:ProjectionType="equirectangular" GPano:UsePanoramaViewer="True" GPano:FullPanoWidthPixels="12000" GPano:FullPanoHeightPixels="6000" GPano:CroppedAreaImageWidthPixels="12000" GPano:CroppedAreaImageHeightPixels="6000" GPano:CroppedAreaLeftPixels="0" GPano:CroppedAreaTopPixels="0""#);
        let p = from_xmp(&dji).unwrap();
        assert!(p.round() && (p.lat_span - PI).abs() < 1e-5 && (p.lon0 + PI).abs() < 1e-5 && (p.lat_top - FRAC_PI_2).abs() < 1e-5, "{p:?}");
        assert_eq!(p.first, View { yaw: 0.0, pitch: 0.0, fov: FIRST_FOV });
        let elements = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GPano="{GPANO}"><GPano:ProjectionType>equirectangular</GPano:ProjectionType><GPano:FullPanoWidthPixels>12000</GPano:FullPanoWidthPixels></rdf:Description></rdf:RDF></x:xmpmeta>"#
        );
        assert_eq!(from_xmp(&elements), Some(p));
        assert_eq!(from_xmp(&xmp(r#"GPano:ProjectionType="equirectangular" GPano:UsePanoramaViewer="False""#)), None);
        assert_eq!(from_xmp(&xmp(r#"GPano:ProjectionType="cylindrical""#)), None);
        assert_eq!(from_xmp(&xmp(r#"GPano:UsePanoramaViewer="True""#)), None);
        assert_eq!(from_xmp(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"/>"#), None);
        for bad in [r#"GPano:FullPanoWidthPixels="0""#, r#"GPano:FullPanoWidthPixels="100" GPano:CroppedAreaImageWidthPixels="200""#, r#"GPano:FullPanoWidthPixels="100" GPano:CroppedAreaLeftPixels="-5""#, r#"GPano:FullPanoWidthPixels="nan""#] {
            assert_eq!(from_xmp(&xmp(&format!(r#"GPano:ProjectionType="equirectangular" {bad}"#))), None, "{bad}");
        }
    }

    /// #188: in a JPEG the XMP is read from its segment, in another file found in its first bytes.
    #[test]
    fn recognised_in_a_file() {
        let packet = xmp(r#"GPano:ProjectionType="equirectangular""#);
        let mut jpeg = vec![0xFF, 0xD8];
        let body = [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), packet.as_bytes()].concat();
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(&body);
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0, 2, 0xFF, 0xD9]);
        assert_eq!(detect(&jpeg), Some(whole()));
        let png = [b"\x89PNG\r\n\x1a\n....iTXtXML:com.adobe.xmp\0\0\0\0\0".as_slice(), packet.as_bytes(), b"...."].concat();
        assert_eq!(detect(&png), Some(whole()));
        assert_eq!(detect(b"\x89PNG\r\n\x1a\nnothing"), None);
    }

    /// #188: a panorama of 180° × 90°, cropped from the middle of a whole one, covers that part
    /// only: the directions outside it have no pixel, and the view stays on it.
    #[test]
    fn a_partial_panorama_covers_its_part() {
        let p = from_xmp(&xmp(r#"GPano:ProjectionType="equirectangular" GPano:FullPanoWidthPixels="3600" GPano:FullPanoHeightPixels="1800" GPano:CroppedAreaImageWidthPixels="1800" GPano:CroppedAreaImageHeightPixels="900" GPano:CroppedAreaLeftPixels="900" GPano:CroppedAreaTopPixels="450""#)).unwrap();
        assert!(!p.round());
        let size = [1800.0, 900.0];
        assert!(close(p.pixel(direction(0.0, 0.0), size).unwrap(), [900.0, 450.0], 0.05));
        assert!(close(p.pixel(direction(deg(-90.0), deg(45.0)), size).unwrap(), [0.0, 0.0], 0.05));
        assert!(close(p.pixel(direction(deg(90.0), deg(-45.0)), size).unwrap(), [1800.0, 900.0], 0.05));
        assert_eq!(p.pixel(direction(deg(120.0), 0.0), size), None);
        assert_eq!(p.pixel(direction(0.0, deg(60.0)), size), None);
        assert_eq!((p.first.yaw, p.first.pitch), (0.0, 0.0), "the middle of the picture");
        let screen = [1600.0, 900.0];
        let v = p.clamped(View { yaw: deg(170.0), pitch: deg(80.0), fov: deg(150.0) }, screen);
        let (half_w, half_h) = (v.across(screen, 0) / 2.0, v.across(screen, 1) / 2.0);
        assert!(v.fov <= p.lon_span + 1e-5, "{v:?}");
        assert!(v.yaw + half_w <= deg(90.0) + 1e-4 && v.pitch + half_h <= deg(45.0) + 1e-4, "{v:?}");
        // The middle of each edge of the screen is on the picture; a corner, turned with the
        // pitch, may go a hair past it.
        for edge in [[800.0, 0.0], [1600.0, 450.0], [800.0, 900.0], [0.0, 450.0]] {
            assert!(p.uv(v.ray(edge, screen)).is_some(), "edge {edge:?} off the picture with {v:?}");
        }
        for corner in [[0.0, 0.0], [1600.0, 0.0], [0.0, 900.0], [1600.0, 900.0]] {
            let (lon, lat) = lon_lat(v.ray(corner, screen));
            assert!(lon.abs() <= deg(91.0) && lat.abs() <= deg(46.0), "corner {corner:?} off the picture with {v:?}");
        }
    }

    /// #188: on a whole panorama 3600 × 1800 (a pixel every 0.1°), the pixel of each direction:
    /// ahead the middle, to the right three quarters across, behind the edges, up the top row,
    /// down the bottom one; just either side of the seam behind, the two edges.
    #[test]
    fn the_pixel_of_each_direction() {
        let (p, size) = (whole(), [3600.0, 1800.0]);
        let px = |lon: f32, lat: f32| p.pixel(direction(deg(lon), deg(lat)), size).unwrap();
        assert!(close(px(0.0, 0.0), [1800.0, 900.0], 0.05));
        assert!(close(px(90.0, 0.0), [2700.0, 900.0], 0.05));
        assert!(close(px(-90.0, 0.0), [900.0, 900.0], 0.05));
        assert!(close(px(45.0, 30.0), [2250.0, 600.0], 0.05));
        assert!(close(px(-30.0, -60.0), [1500.0, 1500.0], 0.05));
        assert!((px(0.0, 90.0)[1]).abs() < 0.05 && (px(0.0, -90.0)[1] - 1800.0).abs() < 0.05);
        assert!(close(px(179.9, 0.0), [3599.0, 900.0], 0.1), "{:?}", px(179.9, 0.0));
        assert!(close(px(-179.9, 0.0), [1.0, 900.0], 0.1), "{:?}", px(-179.9, 0.0));
        let behind = px(180.0, 0.0)[0];
        assert!(behind < 0.05 || behind > 3599.95, "{behind}");
    }

    /// #188: what the middle and the edges of the screen show at a known yaw, pitch and field of
    /// view, and back: a direction projected falls where it is seen.
    #[test]
    fn the_screen_and_the_sphere() {
        let (p, size, screen) = (whole(), [3600.0, 1800.0], [1600.0, 900.0]);
        let view = View { yaw: deg(90.0), pitch: deg(30.0), fov: deg(90.0) };
        assert!(close(p.pixel(view.ray([800.0, 450.0], screen), size).unwrap(), [2700.0, 600.0], 0.1));
        let flat = View { yaw: 0.0, pitch: 0.0, fov: deg(90.0) };
        // At 90° across, the right edge is 45° to the right; the top edge atan(450/800) up.
        assert!(close(p.pixel(flat.ray([1600.0, 450.0], screen), size).unwrap(), [2250.0, 900.0], 0.1));
        let up = (450.0f32 / 800.0).atan().to_degrees();
        assert!(close(p.pixel(flat.ray([800.0, 0.0], screen), size).unwrap(), [1800.0, 900.0 - up * 10.0], 0.1));
        for point in [[0.0, 0.0], [800.0, 450.0], [1234.0, 77.0], [1600.0, 900.0]] {
            let back = view.project(view.ray(point, screen), screen).unwrap();
            assert!(close(back, point, 0.05), "{point:?} {back:?}");
        }
        assert_eq!(view.project(direction(deg(-90.0), deg(-30.0)), screen), None, "behind the eye");
    }

    /// #188: the triangles the viewer draws show, at each point of the screen, the pixel the sums
    /// say, within about a pixel: on a picture of known cells of 10°, the cell under each point
    /// is the one of its direction; at a narrow and a wide view, looking at the seam and up.
    #[test]
    fn the_triangles_show_what_the_sums_say() {
        let (p, size, screen) = (whole(), [8192.0, 4096.0], [1600.0, 900.0]);
        // The cell of the picture at a pixel, as a picture of known cells would show it.
        let cell = |px: [f32; 2]| ((px[0] / size[0] * 36.0).floor().min(35.0) as i32, (px[1] / size[1] * 18.0).floor().min(17.0) as i32);
        for view in [View { yaw: 0.0, pitch: 0.0, fov: deg(90.0) }, View { yaw: deg(180.0), pitch: 0.0, fov: deg(90.0) }, View { yaw: deg(37.0), pitch: deg(70.0), fov: deg(120.0) }, View { yaw: deg(-120.0), pitch: deg(-20.0), fov: NARROWEST }] {
            let g = grid(&p, view, screen);
            let tris: Vec<[usize; 3]> = g.triangles().collect();
            let mut worst = 0.0f32;
            let (mut cells, mut points) = (0, 0);
            for sy in (0..=900).step_by(50) {
                for sx in (0..=1600).step_by(50) {
                    let at = [sx as f32, sy as f32];
                    let Some(uv) = interpolate(&g, &tris, at) else { panic!("{at:?} not covered with {view:?}") };
                    let drawn = [uv[0] * size[0], uv[1] * size[1]];
                    let sums = p.pixel(view.ray(at, screen), size).unwrap();
                    let off = (drawn[0] - sums[0]).abs().min(size[0] - (drawn[0] - sums[0]).abs()).max((drawn[1] - sums[1]).abs());
                    // Near a pole a pixel of the picture is a sliver of the sphere: there the
                    // distance on the sphere counts.
                    let on_sphere = p.at(uv);
                    let apart = dot(on_sphere, view.ray(at, screen)).clamp(-1.0, 1.0).acos() * size[0] / TAU;
                    worst = worst.max(off.min(apart));
                    points += 1;
                    if off < 0.5 && cell(drawn) == cell(sums) {
                        cells += 1;
                    }
                }
            }
            assert!(worst < 2.0, "{view:?}: off by {worst} pixels");
            // Near a pole the columns of cells meet: there a point may fall on the next one.
            assert!(cells * 10 > points * 7, "{view:?}: {cells} of {points}");
        }
        // No triangle goes across the seam: its corners are close together on the picture.
        let g = grid(&p, View { yaw: deg(180.0), pitch: 0.0, fov: deg(90.0) }, screen);
        assert!(g.triangles().all(|t| t.iter().all(|&k| (g.uv[k][0] - g.uv[t[0]][0]).abs() < 0.01)));
    }

    /// The point of the picture drawn at `at`: the triangle of the grid that holds it, its corners
    /// weighed as the graphics card does.
    fn interpolate(g: &Grid, tris: &[[usize; 3]], at: [f32; 2]) -> Option<[f32; 2]> {
        for t in tris {
            let [a, b, c] = t.map(|k| g.screen[k].unwrap());
            let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if det.abs() < 1e-9 {
                continue;
            }
            let w0 = ((b[1] - c[1]) * (at[0] - c[0]) + (c[0] - b[0]) * (at[1] - c[1])) / det;
            let w1 = ((c[1] - a[1]) * (at[0] - c[0]) + (a[0] - c[0]) * (at[1] - c[1])) / det;
            let w2 = 1.0 - w0 - w1;
            if w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4 {
                let [ua, ub, uc] = t.map(|k| g.uv[k]);
                return Some([w0 * ua[0] + w1 * ub[0] + w2 * uc[0], w0 * ua[1] + w1 * ub[1] + w2 * uc[1]]);
            }
        }
        None
    }

    /// #188: a drag moves the picture with the pointer, the wheel's zoom narrows the view, and
    /// the scale of the status bar and the field of view for it agree.
    #[test]
    fn drag_and_zoom() {
        let (p, screen) = (whole(), [1600.0, 900.0]);
        let v = View { yaw: 0.0, pitch: 0.0, fov: deg(90.0) };
        // What was under the pointer at the middle is under it after a drag of 200 px to the right.
        let under = v.ray([800.0, 450.0], screen);
        let moved = v.dragged([200.0, -100.0], screen);
        assert!(close(moved.project(under, screen).unwrap(), [1000.0, 350.0], 2.0), "{:?}", moved.project(under, screen));
        let closer = v.zoomed(2.0);
        assert!((closer.focal(screen) - 2.0 * v.focal(screen)).abs() < 0.01);
        let s = p.scale(v, screen, 8192.0);
        assert!((p.fov_for(s, screen, 8192.0) - v.fov).abs() < 1e-4);
        assert_eq!(p.clamped(View { fov: deg(5.0), ..v }, screen).fov, NARROWEST);
        assert_eq!(p.clamped(View { fov: deg(170.0), ..v }, screen).fov, WIDEST);
        assert!((p.clamped(View { yaw: deg(190.0), ..v }, screen).yaw - deg(-170.0)).abs() < 1e-4, "round and round");
        assert_eq!(p.clamped(View { pitch: deg(100.0), ..v }, screen).pitch, FRAC_PI_2);
    }

    /// #188: a panorama EyeCanSee turned on its side and saved, or cropped, keeps the tags of the
    /// whole one (`xmp.rs` keeps the properties it does not know): its shape is not the one they
    /// say, and it stays flat. One only made smaller is still a panorama.
    #[test]
    fn a_picture_of_another_shape_stays_flat() {
        let p = whole();
        assert!(p.fits([8192.0, 4096.0]) && p.fits([12000.0, 6000.0]) && p.fits([1001.0, 500.0]));
        assert!(!p.fits([4096.0, 8192.0]) && !p.fits([8000.0, 4096.0]) && !p.fits([4096.0, 4096.0]) && !p.fits([100.0, 0.0]));
        let cropped = from_xmp(&xmp(r#"GPano:ProjectionType="equirectangular" GPano:FullPanoWidthPixels="3600" GPano:CroppedAreaImageWidthPixels="1800" GPano:CroppedAreaImageHeightPixels="600""#)).unwrap();
        assert!(cropped.fits([900.0, 300.0]) && !cropped.fits([1800.0, 900.0]));
    }

    /// #188: the first look a file asks for (heading and pitch, in compass degrees), else the
    /// middle of the picture.
    #[test]
    fn the_first_look() {
        let p = from_xmp(&xmp(r#"GPano:ProjectionType="equirectangular" GPano:PoseHeadingDegrees="350" GPano:InitialViewHeadingDegrees="20" GPano:InitialViewPitchDegrees="-10" GPano:InitialHorizontalFOVDegrees="75""#)).unwrap();
        assert!((p.first.yaw - deg(30.0)).abs() < 1e-4 && (p.first.pitch - deg(-10.0)).abs() < 1e-4 && (p.first.fov - deg(75.0)).abs() < 1e-4, "{:?}", p.first);
    }
}
