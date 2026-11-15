//! The sliders of EyeCanSee, all with tick marks under the rail and their measure in sight: a
//! slider without them did not say what it was (Piero, 15/11). Where the value moves by steps
//! (the size of the thumbnails, a whole number of pixels or degrees), the slider stops on them
//! and there is a mark for each step, or a mark every few steps when they are many; where it is
//! continuous (the place of a section, of a video), 5 to 11 marks on round values. The marks are
//! drawn in the colour of the text, so they show in the light and the dark theme and on the white
//! on blue of the status bars. Without egui: where the marks go and the step nearest a value.

use std::ops::RangeInclusive;

use eframe::egui;

/// Most marks on a rail: past this, a mark every few steps.
const MOST: usize = 21;
/// Most intervals between marks on a rail with many steps or none (11 marks).
const MOST_SPANS: usize = 10;
/// Least room between two marks, in points: a short rail has fewer of them.
const LEAST_GAP: f32 = 6.0;

/// The value on the steps of `step` from the start of `range` nearest to `value`, inside it.
pub(crate) fn snap(value: f64, range: RangeInclusive<f64>, step: f64) -> f64 {
    let (lo, hi) = (range.start().min(*range.end()), range.start().max(*range.end()));
    if value.is_nan() {
        return *range.start();
    }
    if step <= 0.0 || value.is_infinite() {
        return value.clamp(lo, hi);
    }
    let n = ((value - range.start()) / step).round();
    (range.start() + n * step).clamp(lo, hi)
}

/// How many marks a rail `length` points long has room for.
pub(crate) fn room(length: f32) -> usize {
    ((length / LEAST_GAP).floor() as usize + 1).clamp(2, MOST)
}

/// A round spacing for the marks over `span`: 1, 2, 2.5 or 5 times a power of ten, giving at most
/// `spans` intervals; with a `step`, a whole number of steps.
fn round_spacing(span: f64, step: Option<f64>, spans: usize) -> f64 {
    let spans = spans.max(1) as f64;
    let mut power = 10f64.powf((span / spans).log10().floor());
    while power <= span * 10.0 {
        for m in [1.0, 2.0, 2.5, 5.0] {
            let s = m * power;
            let whole = step.is_none_or(|step| ((s / step) - (s / step).round()).abs() < 1e-6);
            if whole && span / s <= spans + 1e-9 {
                return s;
            }
        }
        power *= 10.0;
    }
    span
}

/// Where the marks go on `range` with room for `most` of them: with a `step`, one per step when
/// they fit; otherwise, and on a continuous rail, at the multiples of a round spacing, at most 11,
/// with the two ends always marked and no mark nearer than half a spacing to them.
pub(crate) fn marks(range: RangeInclusive<f64>, step: Option<f64>, most: usize) -> Vec<f64> {
    let (lo, hi) = (range.start().min(*range.end()), range.start().max(*range.end()));
    let span = hi - lo;
    if span <= 0.0 || !span.is_finite() {
        return vec![lo];
    }
    let most = most.clamp(2, MOST);
    if let Some(step) = step.filter(|s| *s > 0.0)
        && span / step <= (most - 1) as f64 + 1e-9
    {
        let n = (span / step + 1e-9).floor() as usize;
        let mut v: Vec<f64> = (0..=n).map(|i| lo + i as f64 * step).collect();
        if hi - v[n] > step * 1e-6 {
            v.push(hi);
        }
        return v;
    }
    spaced(lo..=hi, round_spacing(span, step, (most - 1).min(MOST_SPANS)))
}

/// The two ends of `range` (low to high), and the multiples of `s` between them not nearer than
/// half of `s` to an end.
fn spaced(range: RangeInclusive<f64>, s: f64) -> Vec<f64> {
    let (lo, hi) = (*range.start(), *range.end());
    let mut v = vec![lo];
    let mut k = (lo / s).ceil();
    while k * s <= hi - s / 2.0 + 1e-9 {
        if k * s - lo >= s / 2.0 - 1e-9 {
            v.push(k * s);
        }
        k += 1.0;
    }
    v.push(hi);
    v
}

/// Where the marks of a slider go: its range, and the step it stops on (none: continuous).
#[derive(Clone, Debug)]
pub(crate) struct Marks {
    range: RangeInclusive<f64>,
    step: Option<f64>,
    /// A spacing of the marks chosen by the slider (45° for an angle).
    every: Option<f64>,
}

impl Marks {
    pub(crate) fn new<T: egui::emath::Numeric>(range: &RangeInclusive<T>, step: Option<f64>) -> Self {
        Self { range: range.start().to_f64()..=range.end().to_f64(), step, every: None }
    }

    /// A mark every `every` (multiples of it, and the ends).
    pub(crate) fn every(self, every: f64) -> Self {
        Self { every: Some(every), ..self }
    }

    /// The values marked on a rail with room for `most` marks.
    pub(crate) fn values(&self, most: usize) -> Vec<f64> {
        let (lo, hi) = (self.range.start().min(*self.range.end()), self.range.start().max(*self.range.end()));
        match self.every {
            Some(e) if e > 0.0 && hi > lo => spaced(lo..=hi, e),
            _ => marks(self.range.clone(), self.step, most),
        }
    }

    /// Whole numbers: a step of 1.
    pub(crate) fn whole<T: egui::emath::Numeric>(range: &RangeInclusive<T>) -> Self {
        Self::new(range, Some(1.0))
    }
}

/// `slider`, stopping on the steps of `marks` when it has one, with its marks under the rail.
/// The value is not moved to a step by this: where a kept value must be on one (the thumbnails)
/// `snap` brings it there first; in the editor a width with a fraction stays as it is until the
/// user moves the slider.
pub(crate) fn add(ui: &mut egui::Ui, slider: egui::Slider, marks: &Marks) -> egui::Response {
    let slider = match marks.step {
        Some(step) => slider.step_by(step),
        None => slider,
    };
    let response = ui.add(slider);
    let most = room(ui.spacing().slider_width);
    paint(ui, &response, marks.range.clone(), &marks.values(most));
    response
}

/// The marks under the rail of a horizontal slider drawn as `response`, in the text's colour:
/// the rail is the first `slider_width` points of it, centred on its height, and a handle's
/// centre goes from one end of it to the other, inset by its radius, as egui draws it.
pub(crate) fn paint(ui: &egui::Ui, response: &egui::Response, range: RangeInclusive<f64>, marks: &[f64]) {
    let (lo, hi) = (*range.start(), *range.end());
    if hi == lo || !ui.is_rect_visible(response.rect) {
        return;
    }
    let width = ui.spacing().slider_width.min(response.rect.width());
    let thick = ui.text_style_height(&egui::TextStyle::Body).max(ui.spacing().interact_size.y);
    let radius = thick / 2.5;
    let (left, right) = (response.rect.left() + radius, response.rect.left() + width - radius);
    let y = response.rect.center().y;
    let top = y + ui.spacing().slider_rail_height / 2.0 + 1.0;
    let bottom = (y + thick / 2.0 + 1.0).max(top + 4.0);
    let colour = ui.visuals().text_color().gamma_multiply(if response.enabled() { 1.0 } else { 0.5 });
    let stroke = egui::Stroke::new(1.0, colour);
    for &m in marks {
        let x = egui::emath::remap(m, lo..=hi, f64::from(left)..=f64::from(right)) as f32;
        let x = ui.painter().round_to_pixel_center(x);
        ui.painter().vline(x, top..=bottom, stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_step() {
        // The thumbnails: 64 to 256 by 32.
        assert_eq!(snap(160.0, 64.0..=256.0, 32.0), 160.0);
        assert_eq!(snap(170.0, 64.0..=256.0, 32.0), 160.0);
        assert_eq!(snap(176.0, 64.0..=256.0, 32.0), 192.0);
        assert_eq!(snap(10.0, 64.0..=256.0, 32.0), 64.0);
        assert_eq!(snap(999.0, 64.0..=256.0, 32.0), 256.0);
        // Steps from the start, not from 0: 5 to 100 by 5, -180 to 180 by 1.
        assert_eq!(snap(7.4, 5.0..=100.0, 5.0), 5.0);
        assert_eq!(snap(-61.37, -180.0..=180.0, 1.0), -61.0);
        // A range from high to low.
        assert_eq!(snap(3.2, 10.0..=0.0, 1.0), 3.0);
        assert_eq!(snap(f64::NAN, 0.0..=1.0, 0.1), 0.0);
    }

    #[test]
    fn one_mark_per_step() {
        assert_eq!(marks(64.0..=256.0, Some(32.0), 21), vec![64.0, 96.0, 128.0, 160.0, 192.0, 224.0, 256.0]);
        assert_eq!(marks(0.0..=9.0, Some(1.0), 21).len(), 10);
        assert_eq!(marks(1.0..=10.0, Some(1.0), 21).len(), 10);
        // The last step short of the end: the end is marked too.
        assert_eq!(marks(0.0..=10.0, Some(3.0), 21), vec![0.0, 3.0, 6.0, 9.0, 10.0]);
        // The volume, 0 to 100 by 5: 21 marks on a long rail, every 10 on a short one.
        assert_eq!(marks(0.0..=100.0, Some(5.0), 21).len(), 21);
        assert_eq!(marks(0.0..=100.0, Some(5.0), 12), (0..=10).map(|i| f64::from(i) * 10.0).collect::<Vec<_>>());
    }

    #[test]
    fn round_marks() {
        // Continuous: 0 to 1 every 0.1.
        let m = marks(0.0..=1.0, None, 21);
        assert_eq!(m.len(), 11);
        assert!((m[5] - 0.5).abs() < 1e-9);
        // Many whole steps: a mark every few, on round values.
        assert_eq!(marks(0.0..=100.0, Some(1.0), 21), (0..=10).map(|i| f64::from(i) * 10.0).collect::<Vec<_>>());
        assert_eq!(marks(-180.0..=180.0, Some(1.0), 21), vec![-180.0, -150.0, -100.0, -50.0, 0.0, 50.0, 100.0, 150.0, 180.0]);
        // Ends off the round values: marked, and no mark too close to them.
        let m = marks(5.0..=100.0, Some(1.0), 21);
        assert_eq!((m[0], m[1], m[m.len() - 1]), (5.0, 10.0, 100.0));
        for (range, step) in [(0.0..=1.0, None), (5.0..=100.0, Some(1.0)), (1.0..=30.0, Some(1.0)), (-180.0..=180.0, Some(0.1)), (0.0..=3600.0, None), (1.0..=2000.0, Some(1.0)), (0.0..=0.37, None)] {
            for most in [2, 5, 8, 12, 21] {
                let m = marks(range.clone(), step, most);
                assert!(m.len() <= most.max(2) + 1 && m.len() >= 2, "{range:?} {most}: {m:?}");
                assert_eq!((m[0], m[m.len() - 1]), (*range.start(), *range.end()));
                assert!(m.windows(2).all(|w| w[1] > w[0]), "{range:?}: {m:?}");
            }
        }
    }

    /// An angle: every 45°, the ends included.
    #[test]
    fn chosen_spacing() {
        let m = Marks::whole(&(-180.0..=180.0)).every(45.0);
        assert_eq!(m.values(21), (-4..=4).map(|i| f64::from(i) * 45.0).collect::<Vec<_>>());
        // A range not on multiples: the ends, then the multiples not too near them.
        assert_eq!(Marks::whole(&(10.0..=100.0)).every(25.0).values(21), vec![10.0, 25.0, 50.0, 75.0, 100.0]);
    }

    /// A mark every 6 points at least: 21 on a 120-point rail, 11 on 60.
    #[test]
    fn room_on_the_rail() {
        assert_eq!(room(120.0), 21);
        assert_eq!(room(60.0), 11);
        assert_eq!(room(5.0), 2);
    }
}
