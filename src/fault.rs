//! Errors said plainly. The detail of an error (`source::Error`) is often the raw text of OpenDAL
//! or of the system: "PermissionDenied (permanent) at write, context: { …, path: a/b.jpg, … } =>
//! permission denied, source: Permission denied (os error 13)" in the error bar said nothing to
//! the user (2026-11-10). Here the common kinds become a sentence with the file name, the raw text
//! goes to the log. One place for every error shown (`I18n::error`), whatever produced it.

use std::io::ErrorKind;

use crate::i18n::I18n;
use crate::save_dialogs::md;
use crate::source::{self, NOT_FOUND};

/// The error of a file that could not be decoded (Convert…, Resize…): with `name\u{1}raw`
/// (`not_opened`), said as the viewer says it (`I18n::not_opened`).
pub(crate) const NOT_OPENED: &str = "batch-not-opened";

/// The error of the file `name` that could not be decoded, with the library's text `e`.
pub(crate) fn not_opened(name: &str, e: impl std::fmt::Display) -> (&'static str, String) {
    (NOT_OPENED, format!("{name}\u{1}{e}"))
}

/// A file a server stopped sending before its end (`ftp::cut_short`), with `name\u{1}answer`:
/// the sentence with the name, and the server's answer under it as the detail ("Connection
/// error: early eof" said nothing to the user, L17 of 2026-11-21).
pub(crate) const SERVER_CUT: &str = "error-server-cut";

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

/// Ids whose detail is a name or an address, part of the sentence: never put apart as a detail.
const NAMED: [&str; 9] = [
    "error-path",
    NOT_FOUND,
    "error-exists",
    "error-paste-inside",
    "error-too-deep",
    "error-unsupported",
    "error-no-host",
    crate::model_gltf::MISSING_BIN,
    crate::model_gltf::OUTSIDE_BIN,
];

/// Ids with a sentence of a few words (`<id>-brief`), for the lists of the messages (`I18n::brief`).
const BRIEF: [&str; 7] = [
    crate::model_gltf::MISSING_BIN,
    crate::model_gltf::OUTSIDE_BIN,
    crate::model_gltf::DAMAGED_INSIDE,
    crate::model_gltf::INCOMPLETE,
    crate::model_gltf::CUT,
    crate::model_gltf::CUT_SIZES,
    SERVER_CUT,
];

/// A sentence and, apart, a library's text: the text under the sentence as its detail, in
/// brackets on a line of its own (`error_bar::with_detail`); as it is when it has more lines or
/// its own brackets.
fn under((sentence, apart): (String, Option<String>)) -> String {
    match apart {
        Some(raw) if !raw.contains('\n') && !raw.starts_with('(') => format!("{sentence}\n({raw})"),
        Some(raw) => format!("{sentence}\n{raw}"),
        None => sentence,
    }
}

impl I18n {
    /// The text of an error, said plainly when its detail is recognised (`recognise`); its raw
    /// text goes to the log.
    pub(crate) fn error(&self, id: &str, detail: &str) -> String {
        // A GLB cut short, with the two sizes written as the program writes them (#178).
        if let Some(text) = crate::model_gltf::cut_text(self, id, detail) {
            return text;
        }
        if let Some((name, raw)) = detail.split_once('\u{1}').filter(|_| id == NOT_OPENED) {
            return self.not_opened(name, raw).0;
        }
        if let Some((name, raw)) = detail.split_once('\u{1}').filter(|_| id == SERVER_CUT) {
            let text = self.t_args(id, &[("name", name)]);
            return if raw.is_empty() { text } else { format!("{text}\n({raw})") };
        }
        // A detail sent with its own id (`id\u{1}detail`): a picture too large (`pixel_limit`).
        if let Some((inner, d)) = detail.split_once('\u{1}').filter(|(i, _)| crate::pixel_limit::is_too_large(i)) {
            let plain = self.t_args(inner, &[("detail", d)]);
            return if WRAPPERS.contains(&id) { plain } else { self.t_args(id, &[("detail", &plain)]) };
        }
        // Not recognised: the library's text under the sentence, in brackets, as the detail
        // (`error_bar::with_detail`); not a name or an address, which belongs in the sentence.
        let Some((kind, name)) = recognise(detail) else {
            let text = self.t_args(id, &[("detail", detail)]);
            return if NAMED.contains(&id) { text } else { crate::error_bar::with_detail(&text, detail) };
        };
        log!("error {id}, said as {kind:?}: {detail}");
        let plain = self.fault(kind, name.as_deref());
        if WRAPPERS.contains(&id) { plain } else { self.t_args(id, &[("detail", &plain)]) }
    }

    /// A failed load as shown. The fixed sentences the grid recognises (`occt::MISSING`) stay
    /// untranslated where they are kept and are translated here (#102).
    /// An error sent with its Fluent id (`id\u{1}detail`: a picture too large, `pixel_limit`) is
    /// translated too.
    pub(crate) fn load_error(&self, e: &str) -> String {
        match e.split_once('\u{1}') {
            _ if e == crate::occt::MISSING => self.t("cad-engine-missing"),
            Some((id, detail)) => self.error(id, detail),
            None => e.to_string(),
        }
    }

    /// "Cannot open x": why said plainly under it when it is known, otherwise the library's text
    /// as the detail, in brackets on its own line, as `error_bar::with_detail` puts it.
    pub(crate) fn cannot_load(&self, file: &str, e: &str) -> String {
        under(self.cannot_load_apart(file, e))
    }

    /// `not_opened` in one text, as `cannot_load`: the sentence, and the library's text under it
    /// as the detail. For an error about a file that did not open said in the error bar (two
    /// drawings that could not be compared, `compare_failed`).
    pub(crate) fn not_opened_told(&self, name: &str, raw: &str) -> String {
        under(self.not_opened(name, raw))
    }

    /// Why two drawings were not compared (`e`: the name of the one that did not open and the
    /// text of its library, `name\u{1}raw`, or what else went wrong), for the error bar: the
    /// drawing said as the viewer says a file that does not open, the library's text under it as
    /// the detail, not in the sentence ("…: rotto.dwg: Unsupported CAD version: "Questo"", R13
    /// of 2026-11-21).
    pub(crate) fn compare_failed(&self, e: &str) -> String {
        let why = match e.split_once('\u{1}') {
            Some((name, raw)) => self.not_opened_told(name, raw),
            None => under((String::new(), Some(e.trim().to_string()))).trim_start().to_string(),
        };
        self.t_args("compare-failed", &[("error", &why)])
    }

    /// `cannot_load` in two parts: the sentence, with why said plainly under it when it is
    /// known, and apart the library's text when it is not.
    fn cannot_load_apart(&self, file: &str, e: &str) -> (String, Option<String>) {
        let said = self.load_error(e);
        if said != e {
            return (self.t_args("cannot-load", &[("file", file), ("error", &said)]), None);
        }
        let sentence = self.t_args("cannot-load", &[("file", file), ("error", "")]).trim_end().to_string();
        (sentence, Some(e.trim().to_string()).filter(|raw| !raw.is_empty()))
    }

    /// The file `name` that could not be decoded, with the library's text `raw` (`NOT_OPENED`):
    /// the viewer's sentence (`cannot_load_apart`, "Cannot open x"), and apart the library's text, for
    /// Copy and the log ("Error: No decoder found, model '', …" said nothing to the user, 21/11).
    /// A reason the system gave (no permission, a full disk) is said plainly, as everywhere.
    pub(crate) fn not_opened(&self, name: &str, raw: &str) -> (String, Option<String>) {
        if let Some((kind, file)) = recognise(raw) {
            log!("error {NOT_OPENED}, said as {kind:?}: {raw}");
            return (self.fault(kind, Some(file.as_deref().unwrap_or(name))), None);
        }
        self.cannot_load_apart(name, raw)
    }

    /// Why, in a few words, to go after a file's name in a list of a message (`notice_list`: "a.gltf:
    /// the file a.bin is missing"), for the errors that have such words (`BRIEF`) and for what
    /// the system said (`recognise`). `None`: the list has the name alone, Copy the whole text.
    pub(crate) fn brief(&self, id: &str, detail: &str) -> Option<String> {
        if BRIEF.contains(&id) {
            return Some(self.t_args(&format!("{id}-brief"), &[("detail", detail)]));
        }
        recognise(detail).map(|(kind, _)| self.t(&format!("{}-brief", kind.id())))
    }

    /// `brief`, of a failed load as the loader gives it (`load_error`).
    pub(crate) fn brief_load(&self, e: &str) -> Option<String> {
        match e.split_once('\u{1}') {
            _ if e == crate::occt::MISSING => Some(self.t("cad-engine-missing-brief")),
            Some((id, detail)) => self.brief(id, detail),
            None => self.brief("", e),
        }
    }

    /// `error`, to go in a message window's Markdown (`save_dialogs::notice_text`).
    pub(crate) fn error_md(&self, id: &str, detail: &str) -> String {
        match recognise(detail) {
            Some(_) => md(&self.error(id, detail)),
            None if detail.contains('\u{1}') || id == crate::model_gltf::CUT_SIZES => md(&self.error(id, detail)),
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

    /// Compare drawings on a text file renamed `.dwg` (R13 of 2026-11-21), with the error as the
    /// CAD library gives it: the bar says that the drawings were not compared and which one did
    /// not open, as the viewer says it; the library's English stays out of the sentence, under
    /// it as the detail. A drawing that is gone is said plainly; an error without a file too.
    #[test]
    fn drawings_not_compared() {
        let none: crate::model::Sibling = &|_| None;
        let Err(raw) = crate::cad::read_with(b"Questo non e un disegno DWG.\nFile di testo rinominato per la prova.\n", "rotto.dwg", none) else { panic!("a text file read as a drawing") };
        for (lang, lead, open) in [("it", "Non è stato possibile confrontare i disegni", "Impossibile aprire rotto.dwg"), ("en", "The drawings could not be compared", "Cannot open rotto.dwg")] {
            let i18n = I18n::new(lang);
            let text = i18n.compare_failed(&format!("rotto.dwg\u{1}{raw}"));
            assert_eq!(text, format!("{lead}\n{open}\n({raw})"));
            assert_eq!(text, format!("{lead}\n{}", i18n.cannot_load("rotto.dwg", &raw)), "as the viewer says it");
            let p = crate::error_bar::bar_parts(&text);
            assert_eq!((p.lead, p.rest, p.lines, p.detail), (lead, "", vec![open], vec![format!("({raw})").as_str()]));
            assert_eq!(i18n.compare_failed("task 7 panicked"), format!("{lead}\n(task 7 panicked)"));
        }
        let gone = "NotFound (permanent) at read, context: { service: fs, path: disegni/a.dxf } => entry not found";
        let it = I18n::new("it");
        assert_eq!(it.compare_failed(&format!("a.dxf\u{1}{gone}")), format!("Non è stato possibile confrontare i disegni\n{}", it.fault(Kind::NotFound, Some("a.dxf"))));
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

    /// A file a server stopped sending (L17 of 2026-11-21): a sentence with the name, the server's
    /// answer as the detail, in both languages; never "early eof".
    #[test]
    fn server_cut_said_plainly() {
        let detail = "self\u{1}550 Issue during transfer: read /prove/self: is a directory";
        let en = I18n::new("en").error(SERVER_CUT, detail);
        assert_eq!(en, "The server stopped sending \"self\" before its end.\n(550 Issue during transfer: read /prove/self: is a directory)");
        let it = I18n::new("it").error(SERVER_CUT, detail);
        assert!(it.starts_with("Il server ha smesso di inviare \"self\" prima della fine.\n("), "{it}");
        assert_eq!(I18n::new("en").error(SERVER_CUT, "a.jpg\u{1}"), "The server stopped sending \"a.jpg\" before its end.");
        assert!(!I18n::new("it").error_md(SERVER_CUT, detail).contains('\u{1}'));
    }

    #[test]
    fn engine_missing_translated() {
        assert_eq!(I18n::new("en").load_error(crate::occt::MISSING), "The CAD engine is missing.");
        assert_eq!(I18n::new("it").load_error(crate::occt::MISSING), "Manca il motore CAD.");
        assert_eq!(I18n::new("it").load_error("timeout"), "timeout");
        // A picture too large (`pixel_limit`), with its size.
        let e = crate::pixel_limit::check(60000, 60000).unwrap_err();
        assert_eq!(I18n::new("en").load_error(&e), "The picture is too large: 60000 × 60000 pixels, while EyeCanSee opens pictures of up to 512 megapixels.");
        // Also as the detail of another error (a turn saved over the original, `replace::rewritten`).
        assert_eq!(I18n::new("en").error("error-detail", &e), I18n::new("en").load_error(&e));
        assert!(!I18n::new("it").error_md("batch-failed", &e).contains('\u{1}'));
    }

    /// The message of a replace refused for want of room for the second copy (#90), in both
    /// languages: the reason, the name, what to do.
    #[cfg(unix)]
    #[test]
    fn no_space_to_replace() {
        let t = "write, context: { path: .f.jpg.eyecansee-new } => No space left on device (os error 28)";
        let en = I18n::new("en").error("error-detail", t);
        assert_eq!(en, "There is not enough space on the disk to replace \"f.jpg\": while it is being replaced, the disk needs room for a second copy of the file.\nFree some space and try again.");
        let it = I18n::new("it").error("error-detail", t);
        assert_eq!(it, "Sul disco non c'è abbastanza spazio per sostituire \"f.jpg\": durante la sostituzione serve spazio per una seconda copia del file.\nLibera un po' di spazio e riprova.");
        // A full disk while writing the file itself: the plain message.
        let plain = I18n::new("en").error("error-detail", &t.replace(".f.jpg.eyecansee-new", "f.jpg"));
        assert_eq!(plain, I18n::new("en").fault(Kind::NoSpace, Some("f.jpg")));
    }

    /// #207: the `trash` crate on Windows, a folder deleted by another program: the error the
    /// user saw (Italian Windows), the same in English, and "path not found". Known by its code,
    /// which Windows does not translate; on any system, as the text says Windows' code.
    #[test]
    fn windows_trash_not_found() {
        let it = r#"Error during a `trash` operation: Os { code: -2147024894, description: "windows error: Impossibile trovare il file specificato. (0x80070002)" }"#;
        let en = r#"Error during a `trash` operation: Os { code: -2147024894, description: "windows error: The system cannot find the file specified. (0x80070002)" }"#;
        let path = r#"Error during a `trash` operation: Os { code: -2147024893, description: "windows error: Impossibile trovare il percorso specificato. (0x80070003)" }"#;
        for t in [it, en, path] {
            assert_eq!(recognise(t), Some((Kind::NotFound, None)), "{t}");
            for lang in ["en", "it"] {
                let i18n = I18n::new(lang);
                assert_eq!(i18n.error("error-detail", t), i18n.fault(Kind::NotFound, None), "{lang}: {t}");
            }
        }
        // Another error of Windows: as before.
        let other = r#"Error during a `trash` operation: Os { code: -2147467259, description: "windows error: Errore non specificato (0x80004005)" }"#;
        assert_eq!(recognise(other), None);
    }
}
