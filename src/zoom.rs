//! The viewer's zoom counted in the file's own pixels: 100% is one pixel of the file on one pixel
//! of the screen, also for a photo reduced to the texture limit (8192 px) and for an SVG drawn at
//! 2048 px whatever its size. The viewer paints with a scale in screen pixels per texture pixel;
//! here that scale is turned into the file's and back, without egui.

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

/// The scale a zoom asked to `target` from `scale` reaches: within the limits, and stopping at
/// fitted when zooming out from above it (then it goes on below, to 100%, if that is smaller).
pub fn settle(scale: f32, target: f32, b: Bounds) -> f32 {
    let target = target.clamp(b.low, b.max);
    if scale > b.fit && target < b.fit { b.fit } else { target }
}

/// The zoom the status bar shows (1 = 100%): screen pixels per pixel of the file.
pub fn of_file(scale: f32, per: f32) -> f32 {
    scale / per
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
}
