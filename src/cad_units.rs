//! A CAD drawing's unit and size: `$INSUNITS` as a unit, a metric size in a larger unit when it is
//! long, the size written for the status bar.

use crate::cad::Drawing;

/// A unit of length a drawing is shown in: its symbol, the same in every language, and how many
/// of it one drawing unit is (a plot is in 1/40 mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    pub symbol: &'static str,
    pub factor: f64,
}

/// `$INSUNITS` as AutoCAD numbers it; 0 and the rare ones (light years, parsecs…) as unitless.
pub(crate) fn insunits(code: i16) -> Option<Unit> {
    let symbol = match code {
        1 => "in",
        2 => "ft",
        3 => "mi",
        4 => "mm",
        5 => "cm",
        6 => "m",
        7 => "km",
        8 => "µin",
        9 => "mil",
        10 => "yd",
        11 => "Å",
        12 => "nm",
        13 => "µm",
        14 => "dm",
        _ => return None,
    };
    Some(Unit { symbol, factor: 1.0 })
}

/// A metric size of 10000 or more in a larger unit, so that it reads at a glance: a plan of
/// 3540706 mm is 3541 m. Other units stay as they are.
pub fn readable(mut w: f64, mut h: f64, mut unit: &str) -> (f64, f64, &str) {
    while w.max(h) >= 10_000.0 {
        let (k, larger) = match unit {
            "nm" => (1000.0, "µm"),
            "µm" => (1000.0, "mm"),
            "mm" => (1000.0, "m"),
            "cm" => (100.0, "m"),
            "dm" => (10.0, "m"),
            "m" => (1000.0, "km"),
            _ => break,
        };
        (w, h, unit) = (w / k, h / k, larger);
    }
    (w, h, unit)
}

/// "420 × 297 mm": three significant digits or more, trailing zeros dropped, `decimal` as the
/// language writes it.
pub fn size_text(w: f64, h: f64, unit: &str, decimal: &str) -> String {
    let n = |v: f64| {
        let s = match v.abs() {
            a if a >= 100.0 => format!("{v:.0}"),
            a if a >= 10.0 => format!("{v:.1}"),
            _ => format!("{v:.2}"),
        };
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        s.replace('.', decimal)
    };
    format!("{} × {} {unit}", n(w), n(h))
}

impl Drawing {
    /// Width and height of what is drawn, in its unit: for the status bar, in place of the
    /// pixels a picture has (a drawing has none, user 2026-11-02).
    pub fn size(&self) -> Option<(f64, f64)> {
        let (w, h) = match self.extents {
            Some(e) => e,
            None => self.bounds().map(|[x0, y0, x1, y1]| (x1 - x0, y1 - y0))?,
        };
        let k = self.unit.map_or(1.0, |u| u.factor);
        Some((w * k, h * k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_written() {
        assert_eq!(size_text(420.0, 297.0, "mm", ","), "420 × 297 mm");
        assert_eq!(size_text(16.535, 11.69, "in", ","), "16,5 × 11,7 in");
        assert_eq!(size_text(2.5, 0.126, "m", "."), "2.5 × 0.13 m");
        assert_eq!(insunits(4), Some(Unit { symbol: "mm", factor: 1.0 }));
        assert_eq!(insunits(0), None);
        assert_eq!(readable(3540706.0, 2726368.0, "mm"), (3540.706, 2726.368, "m"));
        assert_eq!(readable(25_000.0, 1.0, "m"), (25.0, 0.001, "km"));
        assert_eq!(readable(25_000.0, 1.0, "in"), (25_000.0, 1.0, "in"));
    }
}
