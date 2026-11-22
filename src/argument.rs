//! The address given on the command line, made ready to open or to hand over to the copy of
//! EyeCanSee already running (`single`).

/// The address given on the command line. A local path is made absolute: the instance it is
/// handed to has another working folder.
pub(crate) fn argument(a: &str) -> String {
    if a.contains("://") {
        return a.to_string();
    }
    let a = if cfg!(windows) { quote_left(a) } else { a.to_string() };
    std::path::absolute(&a).map_or(a, |p| p.to_string_lossy().into_owned())
}

/// Windows: a backslash before the closing quote escapes it (`CommandLineToArgvW`), so
/// `"C:\Photos\"` arrives as `C:\Photos"` (#101). No Windows path can hold a `"`: it is dropped,
/// and a drive left alone gets its backslash back (`D:` is the drive's current folder, not its root).
/// cmd and PowerShell's `Start-Process` end the command line with a space, which then stays inside
/// the quotes (`C:\Photos" `): spaces at the end are dropped first, as Windows drops them from paths.
fn quote_left(a: &str) -> String {
    let a = a.trim_end();
    match a.strip_suffix('"') {
        Some(p) if p.ends_with(':') => format!("{p}\\"),
        Some(p) => p.to_string(),
        None => a.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn quote_left_by_windows() {
        use super::quote_left;
        assert_eq!(quote_left(r#"C:\Photos""#), r"C:\Photos");
        assert_eq!(quote_left(r#"D:""#), r"D:\");
        // The space cmd and Start-Process put at the end of the command line.
        assert_eq!(quote_left(r#"C:\Photos" "#), r"C:\Photos");
        assert_eq!(quote_left(r#"D:" "#), r"D:\");
        assert_eq!(quote_left(r#"\\server\share""#), r"\\server\share");
        assert_eq!(quote_left(r"C:\Photos\"), r"C:\Photos\");
        assert_eq!(quote_left(r"C:\Photos"), r"C:\Photos");
    }
}
