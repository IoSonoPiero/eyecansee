//! The desktop icon on Windows (#210): which copy is running, whether the first-start window asks
//! for an icon, whether a shortcut already starts this copy, and the name a new one takes. The
//! rules only, without the system: writing and reading the shortcuts (COM) is Windows' part.
// The first-start window's question and the Windows shortcuts come next and use these rules;
// until then they are only tested.
#![cfg_attr(not(test), expect(dead_code, reason = "used by the #210 question and the Windows shortcuts, not written yet"))]

use std::path::{Path, PathBuf};

/// A copy that can have a desktop icon.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Copy {
    /// The Microsoft Store package: the icon points to the app of the package (its AUMID).
    Store,
    Zip,
    /// Inno Setup: its installer already offered the icon, so no question; the menu entry stays.
    Installer,
    /// The private test copy installed by its script (`avvia.ps1` next to the exe).
    Test,
}

/// Which copy is running, from what the system says: `None` where there is no icon to offer
/// (Linux, a development build in `target`, a test copy started from a throwaway folder).
pub(crate) fn copy_kind(windows: bool, packaged: bool, test_copy: bool, exe: &Path, exists: impl Fn(&Path) -> bool) -> Option<Copy> {
    if !windows {
        return None;
    }
    let dir = exe.parent()?;
    if test_copy {
        return exists(&dir.join("avvia.ps1")).then_some(Copy::Test);
    }
    if packaged {
        return Some(Copy::Store);
    }
    if exe.components().any(|c| c.as_os_str() == "target") {
        return None;
    }
    Some(if exists(&dir.join("unins000.exe")) { Copy::Installer } else { Copy::Zip })
}

/// Whether the first-start window asks for the icon: the installer's copy never (its installer
/// asked), the others when no shortcut on the desktop starts them already.
pub(crate) fn asks(copy: Option<Copy>, icon_present: bool) -> bool {
    matches!(copy, Some(Copy::Store | Copy::Zip | Copy::Test)) && !icon_present
}

/// The name of the icon, without ".lnk".
pub(crate) fn base_name(copy: Copy) -> &'static str {
    if copy == Copy::Test { "EyeCanSee prova" } else { "EyeCanSee" }
}

/// What a shortcut starts.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Target {
    /// A program, by its path.
    Exe(PathBuf),
    /// PowerShell running a script (`-File "<path>"`): how the test copy's icon starts it.
    Script(PathBuf),
    /// The app of a package, by its AppUserModelID (an icon dragged from Start).
    App(String),
}

/// What this copy is, to compare the shortcuts with.
pub(crate) struct Me {
    pub(crate) copy: Copy,
    pub(crate) exe: PathBuf,
    /// The AppUserModelID of the package, for the Store copy.
    pub(crate) aumid: Option<String>,
}

/// A shortcut to PowerShell is read by its arguments: the script it runs.
pub(crate) fn target_of(path: &Path, args: &str) -> Target {
    // The last part after either separator: a Windows path read on any system (the tests).
    let program = path.to_string_lossy().rsplit(['\\', '/']).next().map(str::to_lowercase);
    match (program.as_deref(), script_of(args)) {
        (Some("powershell.exe" | "pwsh.exe"), Some(script)) => Target::Script(script),
        _ => Target::Exe(path.to_path_buf()),
    }
}

/// The path after `-File`, with or without quotes.
fn script_of(args: &str) -> Option<PathBuf> {
    let at = args.to_lowercase().find("-file")? + "-file".len();
    let rest = args[at..].trim_start();
    let path = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => rest.split_whitespace().next()?,
    };
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Windows paths compared as Windows does: without regard to case and to `\` or `/`.
fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

/// Whether `t` starts this copy: the exe itself; for the Store copy its app; for the test copy
/// also the `avvia.ps1` in the folder of its exe (not another one).
pub(crate) fn starts_me(t: &Target, me: &Me) -> bool {
    match t {
        Target::Exe(path) => same_path(path, &me.exe),
        Target::App(id) => me.aumid.as_deref() == Some(id.as_str()),
        Target::Script(script) => me.copy == Copy::Test && me.exe.parent().is_some_and(|dir| same_path(script, &dir.join("avvia.ps1"))),
    }
}

/// A shortcut found with the name a new icon would take.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Found {
    /// It points to a program that is no longer there (a zip folder moved): it can be replaced.
    Broken,
    /// It starts something that works (another copy): never touched.
    Works,
}

/// Where the new icon goes: a free name, or a broken shortcut replaced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Place {
    New(String),
    Replace(String),
}

/// The name of the new icon: `base`, then "`base` (2)", "(3)"… past the shortcuts that work; a
/// broken one with the name is replaced. `found` tells what is on the desktop with a name.
pub(crate) fn place(base: &str, found: impl Fn(&str) -> Option<Found>) -> Place {
    for n in 1.. {
        let name = if n == 1 { base.to_string() } else { format!("{base} ({n})") };
        match found(&name) {
            None => return Place::New(name),
            Some(Found::Broken) => return Place::Replace(name),
            Some(Found::Works) => {}
        }
    }
    unreachable!("names run out")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Forward slashes, which Windows takes too: the tests also run on Linux, where a backslash is no separator.
    fn kind(packaged: bool, test_copy: bool, exe: &str, files: &[&str]) -> Option<Copy> {
        copy_kind(true, packaged, test_copy, Path::new(exe), |p| files.iter().any(|f| p.ends_with(f)))
    }

    /// Store, zip and the installed test copy ask once; the installer, a development build, a test
    /// copy without `avvia.ps1` next to it and Linux never; an icon already there means no
    /// question.
    #[test]
    fn who_is_asked() {
        let zip = kind(false, false, "C:/Tools/EyeCanSee/eyecansee.exe", &[]);
        let installer = kind(false, false, "C:/Users/a/AppData/Local/Programs/EyeCanSee/eyecansee.exe", &["unins000.exe"]);
        let store = kind(true, false, "C:/Program Files/WindowsApps/x/eyecansee.exe", &[]);
        let test = kind(false, true, "C:/Users/a/AppData/Local/Programs/EyeCanSee-prova/eyecansee.exe", &["avvia.ps1"]);
        let throwaway = kind(false, true, "C:/Users/a/Desktop/prova-righe/eyecansee.exe", &[]);
        let dev = kind(false, false, "C:/dev/eyecansee/target/release/eyecansee.exe", &[]);
        let linux = copy_kind(false, false, false, Path::new("/usr/bin/eyecansee"), |_| false);
        assert_eq!((zip, installer, store, test), (Some(Copy::Zip), Some(Copy::Installer), Some(Copy::Store), Some(Copy::Test)));
        assert_eq!((throwaway, dev, linux), (None, None, None));
        for c in [zip, store, test] {
            assert!(asks(c, false), "{c:?}");
            assert!(!asks(c, true), "{c:?} with an icon");
        }
        for c in [installer, throwaway, dev, linux] {
            assert!(!asks(c, false), "{c:?}");
        }
    }

    #[test]
    fn names() {
        assert_eq!(base_name(Copy::Zip), "EyeCanSee");
        assert_eq!(base_name(Copy::Store), "EyeCanSee");
        assert_eq!(base_name(Copy::Test), "EyeCanSee prova");
    }

    /// Exe paths without regard to case and slashes; AUMIDs exactly; for the test copy the
    /// `avvia.ps1` of its own folder only; another program never.
    #[test]
    fn a_shortcut_that_starts_this_copy() {
        let zip = Me { copy: Copy::Zip, exe: r"C:\Tools\EyeCanSee\eyecansee.exe".into(), aumid: None };
        assert!(starts_me(&target_of(Path::new("c:/tools/eyecansee/EYECANSEE.EXE"), ""), &zip));
        assert!(!starts_me(&target_of(Path::new(r"C:\Other\eyecansee.exe"), ""), &zip));
        assert!(!starts_me(&target_of(Path::new(r"C:\Windows\notepad.exe"), ""), &zip));
        let store = Me { copy: Copy::Store, exe: r"C:\Program Files\WindowsApps\x\eyecansee.exe".into(), aumid: Some("Maker.App_abc!App".into()) };
        assert!(starts_me(&Target::App("Maker.App_abc!App".into()), &store));
        assert!(!starts_me(&Target::App("maker.app_abc!app".into()), &store));
        let test = Me { copy: Copy::Test, exe: "C:/P/EyeCanSee-prova/eyecansee.exe".into(), aumid: None };
        let ps = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        let own = target_of(ps, r#"-NoProfile -ExecutionPolicy Bypass -File "C:\P\EyeCanSee-prova\avvia.ps1""#);
        assert_eq!(own, Target::Script(r"C:\P\EyeCanSee-prova\avvia.ps1".into()));
        assert!(starts_me(&own, &test));
        assert!(starts_me(&target_of(ps, r"-file c:\p\eyecansee-prova\AVVIA.ps1"), &test));
        assert!(!starts_me(&target_of(ps, r#"-File "C:\Other\avvia.ps1""#), &test));
        assert!(!starts_me(&own, &zip), "a script starts only the test copy");
        assert_eq!(target_of(ps, "-Command Get-Date"), Target::Exe(ps.into()));
    }

    /// A free name first; a broken shortcut with the name is replaced; past a working one, "(2)",
    /// then "(3)".
    #[test]
    fn the_name_of_a_new_icon() {
        assert_eq!(place("EyeCanSee", |_| None), Place::New("EyeCanSee".into()));
        assert_eq!(place("EyeCanSee", |_| Some(Found::Broken)), Place::Replace("EyeCanSee".into()));
        let one = |n: &str| (n == "EyeCanSee").then_some(Found::Works);
        assert_eq!(place("EyeCanSee", one), Place::New("EyeCanSee (2)".into()));
        let two = |n: &str| ["EyeCanSee", "EyeCanSee (2)"].contains(&n).then_some(Found::Works);
        assert_eq!(place("EyeCanSee", two), Place::New("EyeCanSee (3)".into()));
        let broken_two = |n: &str| match n {
            "EyeCanSee" => Some(Found::Works),
            "EyeCanSee (2)" => Some(Found::Broken),
            _ => None,
        };
        assert_eq!(place("EyeCanSee", broken_two), Place::Replace("EyeCanSee (2)".into()));
    }
}
