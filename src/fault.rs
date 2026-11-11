//! Errors said plainly. The detail of an error (`source::Error`) is often the raw text of OpenDAL
//! or of the system: "PermissionDenied (permanent) at write, context: { …, path: a/b.jpg, … } =>
//! permission denied, source: Permission denied (os error 13)" in the error bar said nothing to
//! the user (2026-11-10). Here the common kinds become a sentence with the file name, the raw text
//! goes to the log. One place for every error shown (`I18n::error`), whatever produced it.

use std::io::ErrorKind;

use crate::i18n::I18n;
use crate::save_dialogs::md;
use crate::source::{self, NOT_FOUND};

/// What went wrong, as the user can act on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Denied,
    ReadOnly,
    NoSpace,
    /// No space for the hidden new version of a file being replaced (#90): the disk needs room
    /// for a second copy of the file while it is replaced.
    NoSpaceToReplace,
    TooLarge,
    NotFound,
    Exists,
    InUse,
    BadName,
}

impl Kind {
    /// The Fluent id of its sentence.
    fn id(self) -> &'static str {
        match self {
            Kind::Denied => "fault-denied",
            Kind::ReadOnly => "fault-read-only",
            Kind::NoSpace => "fault-no-space",
            Kind::NoSpaceToReplace => "fault-no-space-replace",
            Kind::TooLarge => "fault-too-large",
            Kind::NotFound => "fault-not-found",
            Kind::Exists => "fault-exists",
            Kind::InUse => "fault-in-use",
            Kind::BadName => "fault-bad-name",
        }
    }

    fn of_io(kind: ErrorKind) -> Option<Kind> {
        Some(match kind {
            ErrorKind::PermissionDenied => Kind::Denied,
            ErrorKind::ReadOnlyFilesystem => Kind::ReadOnly,
            ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Kind::NoSpace,
            ErrorKind::FileTooLarge => Kind::TooLarge,
            ErrorKind::NotFound => Kind::NotFound,
            ErrorKind::AlreadyExists => Kind::Exists,
            ErrorKind::ResourceBusy | ErrorKind::ExecutableFileBusy => Kind::InUse,
            ErrorKind::InvalidFilename => Kind::BadName,
            _ => return None,
        })
    }

    /// The kind an OpenDAL error starts with ("PermissionDenied (permanent) at write…").
    fn of_dal(text: &str) -> Option<Kind> {
        let (word, rest) = text.split_once(' ')?;
        if !rest.starts_with('(') {
            return None;
        }
        Some(match word {
            "PermissionDenied" => Kind::Denied,
            "NotFound" => Kind::NotFound,
            "AlreadyExists" => Kind::Exists,
            _ => return None,
        })
    }
}

/// The kind of a raw error text and the file it names, when there is one. The code of the
/// system ("(os error 28)") comes first: OpenDAL calls "Unexpected" a full disk or a read-only
/// one. The code is read on the system that gave it, so Windows' codes are Windows' own.
/// A full disk while writing the hidden files of a replace (`source::save_names`) says why a
/// file that fits on the disk did not (#90).
pub(crate) fn recognise(text: &str) -> Option<(Kind, Option<String>)> {
    let mut kind = os_code(text).and_then(of_code).or_else(|| Kind::of_dal(text))?;
    let path = context_path(text);
    let own = path.as_deref().map(source::name).unwrap_or_default();
    if kind == Kind::NoSpace && source::leftover(own) {
        kind = Kind::NoSpaceToReplace;
    }
    Some((kind, Some(source::saved_image(own).to_string()).filter(|n| !n.is_empty())))
}

/// The kind of a system's error code. Windows says that a file is held by another program with
/// codes Rust leaves uncategorized: ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33), and
/// ERROR_UNABLE_TO_REMOVE_REPLACED (1175) from the `ReplaceFileW` of a paste over a file (#90).
fn of_code(code: i32) -> Option<Kind> {
    if cfg!(windows) && matches!(code, 32 | 33 | 1175) {
        return Some(Kind::InUse);
    }
    Kind::of_io(std::io::Error::from_raw_os_error(code).kind())
}

/// N in "(os error N)".
fn os_code(text: &str) -> Option<i32> {
    let (_, after) = text.rsplit_once("(os error ")?;
    after.split_once(')')?.0.trim().parse().ok()
}

/// The path of an OpenDAL error's context: "context: { service: fs, path: a/b, c.jpg, size: 1 }".
/// It ends at the next ", key: " or at " }": a name with a comma stays whole.
fn context_path(text: &str) -> Option<String> {
    let (_, after) = text.split_once("context: { ")?;
    let start = if after.starts_with("path: ") { 6 } else { after.find(", path: ")? + 8 };
    let after = &after[start..];
    let mut end = after.find(" }").unwrap_or(after.len());
    let mut from = 0;
    while let Some(i) = after[from..end].find(", ").map(|i| i + from) {
        let key = after[i + 2..end].split_once(": ").map(|(k, _)| k);
        if key.is_some_and(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_lowercase() || c == '_')) {
            end = i;
            break;
        }
        from = i + 2;
    }
    let path = after[..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Ids whose sentence only wraps the detail: a recognised detail takes their place. With the
/// others ("the original could not be moved to the trash: …") the plain sentence becomes the detail.
const WRAPPERS: [&str; 5] = ["error-detail", NOT_FOUND, "error-path", "error-permission", "error-exists"];

impl I18n {
    /// The text of an error, said plainly when its detail is recognised (`recognise`); its raw
    /// text goes to the log.
    pub(crate) fn error(&self, id: &str, detail: &str) -> String {
        let Some((kind, name)) = recognise(detail) else { return self.t_args(id, &[("detail", detail)]) };
        log!("error {id}, said as {kind:?}: {detail}");
        let plain = self.fault(kind, name.as_deref());
        if WRAPPERS.contains(&id) { plain } else { self.t_args(id, &[("detail", &plain)]) }
    }

    /// A failed load as shown. The fixed sentences the grid recognises (`occt::MISSING`) stay
    /// untranslated where they are kept and are translated here (#102).
    pub(crate) fn load_error(&self, e: &str) -> String {
        if e == crate::occt::MISSING { self.t("cad-engine-missing") } else { e.to_string() }
    }

    /// `error`, to go in a message window's Markdown (`save_dialogs::notice_text`).
    pub(crate) fn error_md(&self, id: &str, detail: &str) -> String {
        match recognise(detail) {
            Some(_) => md(&self.error(id, detail)),
            None => self.t_args(id, &[("detail", &md(detail))]),
        }
    }

    /// "Cannot save: …", with the error said plainly.
    pub(crate) fn save_failed(&self, (id, detail): &source::Error) -> String {
        self.t_args("save-failed", &[("detail", &self.error(id, detail))])
    }

    /// The sentence of `kind`, with the file's name when it is known.
    pub(crate) fn fault(&self, kind: Kind, name: Option<&str>) -> String {
        self.t_args(kind.id(), &[("known", if name.is_some() { "yes" } else { "no" }), ("name", name.unwrap_or_default())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The save into a folder without write permission, as the user saw it (2026-11-10).
    const DENIED: &str = "PermissionDenied (permanent) at write, context: { service_operation: OpenForWrite, service: fs, path: prove/sola-lettura/Orchid (modificata).jpg, size: 2392621, written: 0 } => permission denied, source: Permission denied (os error 13)";

    #[test]
    fn opendal_permission() {
        assert_eq!(recognise(DENIED), Some((Kind::Denied, Some("Orchid (modificata).jpg".into()))));
    }

    #[cfg(unix)]
    #[test]
    fn system_codes_first() {
        // OpenDAL calls a full disk and a read-only one "Unexpected": the system's code says which.
        let full = "Unexpected (temporary) at write, context: { service: fs, path: a/b.jpg } => no storage space, source: No space left on device (os error 28)";
        assert_eq!(recognise(full), Some((Kind::NoSpace, Some("b.jpg".into()))));
        // The new version written under its hidden name: the image's name is said (#90), and why.
        let hidden = full.replace("a/b.jpg", "a/.b.jpg.eyecansee-new");
        assert_eq!(recognise(&hidden), Some((Kind::NoSpaceToReplace, Some("b.jpg".into()))));
        // As the local replace words it (`replace::local::at`).
        let local = "write, context: { path: .b.jpg.eyecansee-new } => No space left on device (os error 28)";
        assert_eq!(recognise(local), Some((Kind::NoSpaceToReplace, Some("b.jpg".into()))));
        let ro = "Unexpected (temporary) at write, context: { path: c.png, service: fs } => read-only filesystem or storage medium, source: Read-only file system (os error 30)";
        assert_eq!(recognise(ro), Some((Kind::ReadOnly, Some("c.png".into()))));
        // A plain system error: the kind, no name.
        assert_eq!(recognise("Permission denied (os error 13)"), Some((Kind::Denied, None)));
        assert_eq!(recognise("File exists (os error 17)"), Some((Kind::Exists, None)));
        assert_eq!(recognise("File too large (os error 27)"), Some((Kind::TooLarge, None)));
    }

    /// A file held by another program, as Windows says it: opening it, or replacing it (#90).
    #[cfg(windows)]
    #[test]
    fn windows_in_use() {
        for code in [32, 33, 1175] {
            let t = format!("replace, context: {{ path: a.jpg }} => in use (os error {code})");
            assert_eq!(recognise(&t), Some((Kind::InUse, Some("a.jpg".into()))), "{code}");
        }
    }

    #[test]
    fn names_with_commas() {
        let t = "NotFound (permanent) at stat, context: { service: fs, path: Holidays, Rome/1, 2.jpg } => entry not found";
        assert_eq!(recognise(t), Some((Kind::NotFound, Some("1, 2.jpg".into()))));
        let t = "AlreadyExists (permanent) at rename, context: { path: x/a, b.jpg, to: y.jpg } => exists";
        assert_eq!(recognise(t), Some((Kind::Exists, Some("a, b.jpg".into()))));
    }

    #[test]
    fn others_untouched() {
        assert_eq!(recognise("timeout"), None);
        assert_eq!(recognise("Unexpected (temporary) at read => connection reset"), None);
        assert_eq!(recognise("photos/a.jpg"), None);
        assert_eq!(recognise("NotFound here"), None);
    }

    #[test]
    fn sentences() {
        let i18n = I18n::new("en");
        let text = i18n.save_failed(&("error-detail", DENIED.into()));
        assert!(text.contains("\"Orchid (modificata).jpg\""), "{text}");
        assert!(!text.contains("PermissionDenied") && !text.contains("os error"), "{text}");
        // An id that only wraps the detail gives way; another keeps its sentence.
        assert_eq!(i18n.error(NOT_FOUND, DENIED), i18n.fault(Kind::Denied, Some("Orchid (modificata).jpg")));
        let trash = i18n.error("error-trash-failed", DENIED);
        assert!(trash.contains("trash") && trash.contains("Orchid") && !trash.contains("PermissionDenied"), "{trash}");
        // Not recognised: as before.
        assert_eq!(i18n.error("error-detail", "timeout"), "timeout");
        for kind in [Kind::Denied, Kind::ReadOnly, Kind::NoSpace, Kind::NoSpaceToReplace, Kind::TooLarge, Kind::NotFound, Kind::Exists, Kind::InUse, Kind::BadName] {
            for lang in ["en", "it"] {
                let i18n = I18n::new(lang);
                let (named, unnamed) = (i18n.fault(kind, Some("x.jpg")), i18n.fault(kind, None));
                assert!(named.contains("x.jpg") && !unnamed.contains("x.jpg") && named != unnamed, "{lang} {kind:?}");
            }
        }
    }

    #[test]
    fn engine_missing_translated() {
        assert_eq!(I18n::new("en").load_error(crate::occt::MISSING), "The CAD engine is missing.");
        assert_eq!(I18n::new("it").load_error(crate::occt::MISSING), "Manca il motore CAD.");
        assert_eq!(I18n::new("it").load_error("timeout"), "timeout");
    }

    /// The message of a replace refused for want of room for the second copy (#90), in both
    /// languages: the reason, the name, what to do.
    #[cfg(unix)]
    #[test]
    fn no_space_to_replace() {
        let t = "write, context: { path: .f.jpg.eyecansee-new } => No space left on device (os error 28)";
        let en = I18n::new("en").error("error-detail", t);
        assert_eq!(en, "There is not enough space on the disk to replace \"f.jpg\": while it is being replaced, the disk needs room for a second copy of the file. Free some space and try again.");
        let it = I18n::new("it").error("error-detail", t);
        assert_eq!(it, "Sul disco non c'è abbastanza spazio per sostituire \"f.jpg\": durante la sostituzione serve spazio per una seconda copia del file. Libera un po' di spazio e riprova.");
        // A full disk while writing the file itself: the plain message.
        let plain = I18n::new("en").error("error-detail", &t.replace(".f.jpg.eyecansee-new", "f.jpg"));
        assert_eq!(plain, I18n::new("en").fault(Kind::NoSpace, Some("f.jpg")));
    }
}
