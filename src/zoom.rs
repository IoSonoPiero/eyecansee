//! The viewer's zoom counted in the file's own pixels: 100% is one pixel of the file on one pixel
//! of the screen, also for a photo reduced to the texture limit (8192 px) and for an SVG drawn at
//! 2048 px whatever its size. The viewer paints with a scale in screen pixels per texture pixel;
//! here that scale is turned into the file's and back, without egui. Also the zoom of the
//! viewer's status bar (issue #152): the percentage typed in it and its slider, on a logarithmic
//! scale with marks on round values.

/// File pixels one texture pixel stands for: 1 for most pictures, more for a photo reduced to the
/// texture limit, less for an SVG smaller than the size it is drawn at. `own` is the picture's own
/// size when it is not the texture's (`decode::own_size`); the long sides are compared, as both
/// were reduced or drawn keeping the proportions.
pub fn file_per_texture(own: Option<[u32; 2]>, texture: [usize; 2]) -> f32 {
    let texture = texture[0].max(texture[1]);
    match own {
        Some([w, h]) if w.max(h) > 0 && texture > 0 => w.max(h) as f32 / texture as f32,
        _ => 1.0,
    }
}

/// The zoom's limits, as scales in screen pixels per texture pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Fitted to the window, never enlarging the texture: a small picture stays at its size, an
    /// SVG fills the window up to the 2048 px it is drawn at.
    pub fit: f32,
    /// 100%: one pixel of the file on one pixel of the screen.
    pub actual: f32,
    /// Smallest: fitted, or 100% when that is smaller (an SVG smaller than the window).
    pub low: f32,
    /// Largest: `max_zoom` percent of the file's pixels, or of the 2048 px an SVG smaller than
    /// that is drawn at (its own pixels would stop it before fitted: a 24 px icon fitted is
    /// already ~4000%); never less than fitted.
    pub max: f32,
}

/// `window_fit`: the scale at which the texture just fills the window; `per`: `file_per_texture`;
/// `max_zoom`: the Options' largest zoom, in percent of the file's pixels.
pub fn bounds(window_fit: f32, per: f32, max_zoom: u32) -> Bounds {
    let fit = window_fit.min(1.0);
    let actual = per;
    Bounds { fit, actual, low: fit.min(actual), max: (max_zoom as f32 / 100.0 * per.max(1.0)).max(fit) }
}

/// The smallest zoom below fitted, in percent of the file's pixels, as XnView MP allows (#152).
pub const LEAST_ZOOM: f32 = 0.01;
/// The fewest screen pixels the short side of a picture made smaller than fitted keeps, as GNOME's
/// viewer, Loupe (its `SMALLEST_RENDERED_PIXEL_SIZE`): a long panorama stops before it is a line.
pub const LEAST_SIDE: f32 = 16.0;

/// `b` with its smallest zoom below fitted (#152): 1% of the file's pixels, or less small when the
/// short side of the picture, `short` texture pixels, would go under `LEAST_SIDE` screen pixels;
/// never above fitted or 100%, the smallest before. A drawing keeps `bounds` as it is.
pub fn below_fit(b: Bounds, per: f32, short: f32) -> Bounds {
    let least = if short > 0.0 { (LEAST_ZOOM * per).max(LEAST_SIDE / short) } else { b.low };
    Bounds { low: b.low.min(least), ..b }
}

/// The scale a zoom asked to `target` from `scale` reaches: within the limits, and stopping at
/// fitted when zooming out from above it (then it goes on below, down to the smallest).
pub fn settle(scale: f32, target: f32, b: Bounds) -> f32 {
    let target = target.clamp(b.low, b.max);
    if scale > b.fit && target < b.fit { b.fit } else { target }
}

/// The scale a zoom typed or set with the slider reaches: only kept within the limits, as it
/// goes straight to the value and does not stop at fitted on the way.
pub fn reach(target: f32, b: Bounds) -> f32 {
    target.clamp(b.low, b.max)
}

/// The zoom the status bar shows (1 = 100%): screen pixels per pixel of the file.
pub fn of_file(scale: f32, per: f32) -> f32 {
    scale / per
}

/// What the zoom group of a status bar asks for (`zoom_bar::group`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ask {
    /// A zoom typed or set with the slider, of the file's pixels (1 = 100%).
    To(f32),
    /// The − and + buttons, as the − and + keys.
    Out,
    In,
}

/// What the viewer's status bar knows of the zoom and asks of it.
#[derive(Debug, Default, Clone, Copy)]
pub struct Bar {
    /// The smallest and the largest zoom of the open picture, of its file's pixels (1 = 100%),
    /// from the last pass of the viewer; the slider goes from one to the other.
    pub range: [f32; 2],
    /// What the status bar asked, for the viewer to do.
    pub ask: Option<Ask>,
}

/// The zoom typed in the status bar, of the file's pixels (`37`, `37%`, ` 37,5 % ` are 0.37 and
/// 0.375): a number not below 0, with `%` or not, `.` or `,` before the decimals. Not a number:
/// none. Out of the limits is for `reach` to bring back (0 is the smallest).
pub fn typed(text: &str) -> Option<f32> {
    let text = text.trim();
    let number = text.strip_suffix('%').unwrap_or(text).trim().replace(',', ".");
    let v: f32 = number.parse().ok()?;
    (v.is_finite() && v >= 0.0).then_some(v / 100.0)
}

/// A zoom as the status bar shows it: whole percentages, one decimal under 10% (1.5%, written
/// with the language's `decimal`), never 0%.
pub fn percent_text(zoom: f32, decimal: &str) -> String {
    let p = zoom * 100.0;
    if p < 9.95 {
        let s = format!("{p:.1}");
        let s = s.strip_suffix(".0").unwrap_or(&s).replace('.', decimal);
        format!("{s}%")
    } else {
        format!("{p:.0}%")
    }
}

/// Where `zoom` is on a slider from `lo` to `hi` (0 to 1): on a logarithmic scale, so 10% to
/// 100% takes as much room as 100% to 1000%, as the zoom moves by factors.
pub fn to_slider(zoom: f32, [lo, hi]: [f32; 2]) -> f32 {
    if hi <= lo || lo <= 0.0 {
        return 0.0;
    }
    ((zoom / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0)
}

/// The zoom at `at` on a slider from `lo` to `hi`, the opposite of `to_slider`, rounded as it is
/// shown (`percent_text`): a whole percentage, a tenth of one under 10%.
pub fn from_slider(at: f32, [lo, hi]: [f32; 2]) -> f32 {
    if hi <= lo || lo <= 0.0 {
        return lo;
    }
    let zoom = lo * (hi / lo).powf(at.clamp(0.0, 1.0));
    let p = zoom * 100.0;
    let p = if p < 9.95 { (p * 10.0).round() / 10.0 } else { p.round() };
    (p / 100.0).clamp(lo, hi)
}

/// Round zooms marked on the slider, as in Windows Photos (10%, 25%, 50%, 100%, 200%, 400%…).
const MARKED: [f32; 13] = [0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];
/// No mark nearer than this to an end of the slider (of its length): it would look like the end.
const NEAR_END: f32 = 0.04;

/// The marks of the zoom slider from `lo` to `hi`, as zooms: its two ends (the smallest and the
/// largest of the Options) and the round zooms between them, not too near an end.
pub fn slider_marks(range: [f32; 2]) -> Vec<f32> {
    let [lo, hi] = range;
    if hi <= lo || lo <= 0.0 {
        return vec![lo];
    }
    let inner = MARKED.iter().copied().filter(|&z| {
        let at = (z / lo).ln() / (hi / lo).ln();
        (NEAR_END..=1.0 - NEAR_END).contains(&at)
    });
    std::iter::once(lo).chain(inner).chain(std::iter::once(hi)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4 * b.abs().max(1.0)
    }

    #[test]
    fn normal_picture_unchanged() {
        // 4000 × 3000 in a window of a third of it: as before, 100% is the texture's pixel.
        let per = file_per_texture(None, [4000, 3000]);
        assert_eq!(per, 1.0);
        let b = bounds(0.3, per, 2000);
        assert_eq!(b, Bounds { fit: 0.3, actual: 1.0, low: 0.3, max: 20.0 });
        assert!(near(of_file(b.fit, per), 0.3));
        // A small picture is not enlarged to fit.
        assert_eq!(bounds(3.0, per, 2000).fit, 1.0);
    }

    #[test]
    fn photo_beyond_texture_limit() {
        // 12000 × 8000 shown as 8192 × 5461.
        let per = file_per_texture(Some([12000, 8000]), [8192, 5461]);
        assert!(near(per, 12000.0 / 8192.0));
        let b = bounds(0.2, per, 2000);
        // Fitted is a fifth of the texture, so less of the file.
        assert!(near(of_file(b.fit, per), 0.2 * 8192.0 / 12000.0));
        // 100% enlarges the texture: 12000 screen pixels across.
        assert!(near(b.actual * 8192.0, 12000.0));
        assert!(near(of_file(b.max, per), 20.0));
        assert!(near(settle(b.fit, 1000.0, b) * 8192.0, 20.0 * 12000.0));
        assert_eq!(b.low, b.fit);
    }

    #[test]
    fn large_svg() {
        // Declares 10000 × 5000, drawn at 2048 × 1024, in a 1920 × 1080 window.
        let per = file_per_texture(Some([10000, 5000]), [2048, 1024]);
        let b = bounds((1920.0f32 / 2048.0).min(1080.0 / 1024.0), per, 2000);
        assert!(near(of_file(b.fit, per), 0.9375 * 2048.0 / 10000.0));
        assert!(near(b.actual * 2048.0, 10000.0));
        assert!(near(of_file(b.max, per), 20.0));
        assert!(near(settle(b.fit, 1000.0, b) * 2048.0, 20.0 * 10000.0));
    }

    #[test]
    fn small_svg() {
        // A 24 px icon drawn at 2048 px, in a 1920 × 1080 window: fitted it fills the height,
        // 1080 / 24 = 4500% of its pixels, beyond the 2000% of the Options.
        let per = file_per_texture(Some([24, 24]), [2048, 2048]);
        let b = bounds(1080.0 / 2048.0, per, 2000);
        assert!(near(of_file(b.fit, per), 45.0));
        // 100% is 24 screen pixels, smaller than fitted, and the zoom may go down to it.
        assert!(near(b.actual * 2048.0, 24.0));
        assert_eq!(b.low, b.actual);
        // The largest zoom counts on the 2048 px it is drawn at: 2000% of those, beyond fitted,
        // so + enlarges it further, up to 20 × 2048 screen pixels (1706667% of its own).
        assert_eq!(b.max, 20.0);
        assert!(near(settle(b.fit, b.fit * 1.5, b), b.fit * 1.5));
        assert_eq!(settle(b.fit * 15.0, b.fit * 1500.0, b), 20.0);
        assert!(near(of_file(b.max, per), 20.0 * 2048.0 / 24.0));
    }

    #[test]
    fn zooming_out_stops_at_fitted() {
        let per = file_per_texture(Some([24, 24]), [2048, 2048]);
        let b = bounds(1080.0 / 2048.0, per, 2000);
        // From above fitted: stops there; from fitted: goes on to 100%, not below.
        assert_eq!(settle(b.fit * 1.2, b.fit / 1.5, b), b.fit);
        assert!(near(settle(b.fit, b.fit / 1.5, b), b.fit / 1.5));
        assert_eq!(settle(b.actual * 1.2, b.actual / 1.5, b), b.actual);
        // A normal picture never goes below fitted.
        let normal = bounds(0.3, 1.0, 2000);
        assert_eq!(settle(0.3, 0.2, normal), 0.3);
        assert_eq!(settle(0.5, 1.0, normal), 1.0);
    }

    #[test]
    fn below_fitted() {
        // A 3000 × 2000 photo fitted at 62%: down to 1% of its pixels, 30 × 20 on screen.
        let b = below_fit(bounds(0.62, 1.0, 2000), 1.0, 2000.0);
        assert!(near(b.low, 0.01));
        assert_eq!((b.fit, b.max), (0.62, 20.0));
        // Zooming out from above stops at fitted once, then goes on below it.
        assert_eq!(settle(0.8, 0.5, b), 0.62);
        assert!(near(settle(0.62, 0.62 / 1.5, b), 0.62 / 1.5));
        assert_eq!(settle(0.011, 0.005, b), b.low);
        // A panorama 20000 × 400: its short side keeps 16 px, 4%.
        let b = below_fit(bounds(0.09, 1.0, 2000), 1.0, 400.0);
        assert!(near(b.low, 0.04));
        // A photo beyond the texture limit: 1% of the file's pixels, not of the texture's.
        let per = file_per_texture(Some([12000, 8000]), [8192, 5461]);
        let b = below_fit(bounds(0.2, per, 2000), per, 5461.0);
        assert!(near(of_file(b.low, per), 0.01));
        // A small picture (an icon of 12 px): never below what it was, 100%.
        let b = below_fit(bounds(50.0, 1.0, 2000), 1.0, 12.0);
        assert_eq!(b.low, 1.0);
        // A 24 px picture: down to 16 px, as a 24 px SVG icon drawn at 2048 px (below its 100%).
        assert!(near(below_fit(bounds(40.0, 1.0, 2000), 1.0, 24.0).low, 16.0 / 24.0));
        let per = file_per_texture(Some([24, 24]), [2048, 2048]);
        let b = below_fit(bounds(1080.0 / 2048.0, per, 2000), per, 2048.0);
        assert!(near(b.low * 2048.0, 16.0));
    }

    #[test]
    fn typed_zoom() {
        assert_eq!(typed("37"), Some(0.37));
        assert_eq!(typed("37%"), Some(0.37));
        assert_eq!(typed(" 37 % "), Some(0.37));
        assert_eq!(typed("37,5"), Some(0.375));
        assert_eq!(typed("2.5%"), Some(0.025));
        assert_eq!(typed("abc"), None);
        assert_eq!(typed(""), None);
        assert_eq!(typed("%"), None);
        assert_eq!(typed("-5"), None);
        assert_eq!(typed("inf"), None);
        // Out of the limits: brought to them by `reach`.
        let b = below_fit(bounds(0.62, 1.0, 2000), 1.0, 2000.0);
        assert_eq!(typed("0").map(|z| reach(z, b)), Some(b.low));
        assert_eq!(typed("99999").map(|z| reach(z, b)), Some(20.0));
        assert_eq!(typed("37").map(|z| reach(z, b)), Some(0.37));
        // Straight to the value, also from above fitted to below it.
        assert_eq!(reach(0.1, b), 0.1);
    }

    #[test]
    fn shown_percent() {
        assert_eq!(percent_text(0.37, ","), "37%");
        assert_eq!(percent_text(1.0, "."), "100%");
        assert_eq!(percent_text(20.0, "."), "2000%");
        assert_eq!(percent_text(0.015, ","), "1,5%");
        assert_eq!(percent_text(0.01, ","), "1%");
        assert_eq!(percent_text(0.0996, "."), "10%");
        assert_eq!(percent_text(0.374, "."), "37%");
    }

    #[test]
    fn slider_is_logarithmic() {
        let range = [0.01, 20.0];
        assert_eq!(to_slider(0.01, range), 0.0);
        assert_eq!(to_slider(20.0, range), 1.0);
        // 10% to 100% takes as much room as 100% to 1000%.
        let (a, b, c) = (to_slider(0.1, range), to_slider(1.0, range), to_slider(10.0, range));
        assert!(near(b - a, c - b));
        // Out of the range: at the ends.
        assert_eq!((to_slider(0.001, range), to_slider(50.0, range)), (0.0, 1.0));
        // And back, rounded as shown: whole percentages, tenths under 10%.
        for z in [0.01, 0.025, 0.37, 1.0, 4.0, 20.0] {
            assert!(near(from_slider(to_slider(z, range), range), z), "{z}");
        }
        assert_eq!(from_slider(to_slider(0.3737, range), range), 0.37);
        assert!(near(from_slider(to_slider(0.04321, range), range), 0.043));
        // Never out of the range by the rounding.
        assert_eq!(from_slider(0.0, [0.0123, 20.0]), 0.0123);
        // A range of one value (fitted is 100% and the largest): no slider to move.
        assert_eq!(to_slider(1.0, [1.0, 1.0]), 0.0);
        assert_eq!(from_slider(0.5, [1.0, 1.0]), 1.0);
    }

    #[test]
    fn slider_marks_on_round_zooms() {
        // From 1% to the 2000% of the Options: the ends and the round zooms between, 1600% too
        // near the end.
        let m = slider_marks([0.01, 20.0]);
        assert_eq!(m, vec![0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 20.0]);
        // Up to 10000%: 6400% marked.
        assert_eq!(slider_marks([0.04, 100.0]).last(), Some(&100.0));
        assert!(slider_marks([0.04, 100.0]).contains(&64.0));
        // Ends off the round zooms: marked, and in order.
        for range in [[0.0123, 20.0], [0.04, 100.0], [0.3, 1.0], [1.0, 20.0]] {
            let m = slider_marks(range);
            assert_eq!((m[0], m[m.len() - 1]), (range[0], range[1]));
            assert!(m.windows(2).all(|w| w[1] > w[0]), "{range:?}: {m:?}");
        }
    }
}
