//! Sizes, dates and times as the user reads them.

use crate::App;

impl App {
    /// Modification date in the computer's time zone, in the language's format. `modified` stays
    /// as it arrived (the thumbnail cache needs it): Unix seconds from phones, RFC 3339 from OpenDAL.
    pub(crate) fn file_date(&self, modified: &str) -> Option<String> {
        let local = parse_modified(modified)?.to_zoned(jiff::tz::TimeZone::system());
        Some(local.strftime(&self.i18n.t("date-format")).to_string())
    }

    /// 850 KB, 3.9 MB (the language's decimal separator).
    pub(crate) fn human_size(&self, bytes: u64) -> String {
        let kb = bytes as f64 / 1024.0;
        let s = if kb < 1024.0 { format!("{kb:.0} KB") } else { format!("{:.1} MB", kb / 1024.0) };
        s.replace('.', &self.i18n.t("decimal-separator"))
    }
}

/// `File::modified` as an instant: Unix seconds (phones) or RFC 3339 (OpenDAL); "" = no date.
pub(crate) fn parse_modified(modified: &str) -> Option<jiff::Timestamp> {
    match modified.parse::<i64>() {
        Ok(secs) => jiff::Timestamp::from_second(secs).ok(),
        Err(_) => modified.parse().ok(),
    }
}

/// "40 s", "1 min 15 s", "2 h 5 min": the time left of a copy.
pub(crate) fn human_time(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs} s"),
        60..3600 => format!("{} min {} s", secs / 60, secs % 60),
        _ => format!("{} h {} min", secs / 3600, secs % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn human_times() {
        assert_eq!(super::human_time(40), "40 s");
        assert_eq!(super::human_time(75), "1 min 15 s");
        assert_eq!(super::human_time(7500), "2 h 5 min");
    }

    #[test]
    fn modified_formats() {
        let unix = super::parse_modified("1776333600").unwrap();
        let rfc = super::parse_modified("2026-04-16T10:00:00Z").unwrap();
        assert_eq!(unix, rfc);
        assert!(super::parse_modified("2026-04-16T10:00:00.123456789Z").is_some());
        assert!(super::parse_modified("").is_none());
    }
}
