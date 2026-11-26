//! Sizes, dates and times as the user reads them.

use crate::App;
use crate::i18n::I18n;

impl App {
    /// Modification date in the language's format (`date`).
    pub(crate) fn file_date(&self, modified: &str) -> Option<String> {
        date(&self.i18n, modified)
    }

    /// 850 KB, 3.9 MB (`size`).
    pub(crate) fn human_size(&self, bytes: u64) -> String {
        size(&self.i18n, bytes)
    }
}

/// Modification date in the computer's time zone, in the language's format. `modified` stays
/// as it arrived (the thumbnail cache needs it): see `parse_modified`.
pub(crate) fn date(i18n: &I18n, modified: &str) -> Option<String> {
    let local = parse_modified(modified)?.to_zoned(jiff::tz::TimeZone::system());
    Some(local.strftime(&i18n.t("date-format")).to_string())
}

/// 850 KB, 3.9 MB (the language's decimal separator).
pub(crate) fn size(i18n: &I18n, bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    let s = if kb < 1024.0 { format!("{kb:.0} KB") } else { format!("{:.1} MB", kb / 1024.0) };
    s.replace('.', &i18n.t("decimal-separator"))
}

/// `size`, in GB from 1 GB on (6.0 GB, not 6144.0 MB): where big files are the matter, as the
/// window of the phone's own copies (#354). From where `size` would write "1024.0 MB".
pub(crate) fn size_gb(i18n: &I18n, bytes: u64) -> String {
    if bytes as f64 / f64::from(1u32 << 20) >= 1023.95 { crate::view_memory::gigabytes(i18n, bytes) } else { size(i18n, bytes) }
}

/// `File::modified` as an instant: Unix seconds (servers, a phone that writes its time zone),
/// RFC 3339 (OpenDAL) or a date without a zone (a phone's clock, `mtp::phone_modified`), read
/// in the computer's time zone; "" = no date.
pub(crate) fn parse_modified(modified: &str) -> Option<jiff::Timestamp> {
    parse_modified_in(modified, &jiff::tz::TimeZone::system())
}

/// `parse_modified`, a date without a zone read in `tz`.
pub(crate) fn parse_modified_in(modified: &str, tz: &jiff::tz::TimeZone) -> Option<jiff::Timestamp> {
    if let Ok(secs) = modified.parse::<i64>() {
        return jiff::Timestamp::from_second(secs).ok();
    }
    let civil = || modified.parse::<jiff::civil::DateTime>().ok()?.to_zoned(tz.clone()).ok().map(|z| z.timestamp());
    modified.parse().ok().or_else(civil)
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

    /// KB and MB as everywhere, GB from 1 GB on, and never "1024.0 MB" just under it.
    #[test]
    fn sizes_with_gigabytes() {
        let (en, it) = (crate::i18n::I18n::new("en"), crate::i18n::I18n::new("it"));
        const GB: u64 = 1 << 30;
        assert_eq!(super::size_gb(&it, 6 * GB), "6,0 GB");
        assert_eq!(super::size_gb(&en, GB + GB / 2), "1.5 GB");
        assert_eq!(super::size_gb(&en, 850 << 10), "850 KB");
        assert_eq!(super::size_gb(&it, 500 << 20), "500,0 MB");
        assert_eq!(super::size_gb(&en, GB - 1), "1.0 GB");
        assert_eq!(super::size_gb(&en, 1023 << 20), "1023.0 MB");
    }

    #[test]
    fn modified_formats() {
        let unix = super::parse_modified("1776333600").unwrap();
        let rfc = super::parse_modified("2026-04-16T10:00:00Z").unwrap();
        assert_eq!(unix, rfc);
        assert!(super::parse_modified("2026-04-16T10:00:00.123456789Z").is_some());
        // A phone's clock, without its zone (`mtp::phone_modified`).
        assert!(super::parse_modified("2026-11-24T10:30:05").is_some());
        assert!(super::parse_modified("").is_none());
    }
}
