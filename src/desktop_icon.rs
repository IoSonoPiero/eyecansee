//! The desktop icon on Windows (#210): which copy is running, whether the first-start window asks
//! for an icon, whether a shortcut already starts this copy, the name a new one takes and what it
//! starts. The rules are here, without the system; reading and writing the shortcuts (COM) is
//! `desktop_icon_win.rs`, on a thread of its own.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(windows)]
use crate::desktop_icon_win as shell;

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
    #[cfg_attr(not(any(windows, test)), expect(dead_code, reason = "read by desktop_icon_win.rs"))]
    Script(PathBuf),
    /// The app of a package, by its AppUserModelID (an icon dragged from Start).
    App(String),
    /// A shortcut that could not be read: it counts as working, so it is never replaced.
    #[cfg_attr(not(any(windows, test)), expect(dead_code, reason = "made by desktop_icon_win.rs"))]
    Unread,
}

/// What this copy is, to compare the shortcuts with.
pub(crate) struct Me {
    pub(crate) copy: Copy,
    pub(crate) exe: PathBuf,
    /// The AppUserModelID of the package, for the Store copy.
    pub(crate) aumid: Option<String>,
}

/// A shortcut to PowerShell is read by its arguments: the script it runs.
#[cfg_attr(not(any(windows, test)), expect(dead_code, reason = "read by desktop_icon_win.rs"))]
pub(crate) fn target_of(path: &Path, args: &str) -> Target {
    // The last part after either separator: a Windows path read on any system (the tests).
    let program = path.to_string_lossy().rsplit(['\\', '/']).next().map(str::to_lowercase);
    match (program.as_deref(), script_of(args)) {
        (Some("powershell.exe" | "pwsh.exe"), Some(script)) => Target::Script(script),
        _ => Target::Exe(path.to_path_buf()),
    }
}

/// The path after `-File`, with or without quotes.
#[cfg_attr(not(any(windows, test)), expect(dead_code, reason = "read by desktop_icon_win.rs"))]
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
        Target::Unread => false,
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

/// A shortcut found on a desktop, by its name without ".lnk".
#[derive(Clone, Debug)]
pub(crate) struct Link {
    pub(crate) name: String,
    pub(crate) target: Target,
    /// What it starts is there (`works`).
    pub(crate) works: bool,
    /// On the public desktop (an installer run for all users): never replaced, it needs the rights
    /// of an administrator.
    pub(crate) public: bool,
}

/// Whether what a shortcut starts is there. A shortcut to a Store app counts as working: there
/// is no cheap way to tell that its package is gone.
#[cfg_attr(not(any(windows, test)), expect(dead_code, reason = "read by desktop_icon_win.rs"))]
pub(crate) fn works(t: &Target, exists: impl Fn(&Path) -> bool) -> bool {
    match t {
        Target::Exe(path) | Target::Script(path) => exists(path),
        Target::App(_) | Target::Unread => true,
    }
}

/// What the desktop calls for.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Outcome {
    /// A shortcut on either desktop already starts this copy: nothing to ask, nothing to make.
    There,
    Write(Place),
}

/// What to do for this copy with these shortcuts on the two desktops. A name taken on the public
/// desktop, or by a working shortcut, is skipped; one taken only by broken shortcuts of the
/// user's desktop is reused.
pub(crate) fn decide(me: &Me, links: &[Link]) -> Outcome {
    if links.iter().any(|l| starts_me(&l.target, me)) {
        return Outcome::There;
    }
    Outcome::Write(place(base_name(me.copy), |name| {
        let mut same = links.iter().filter(|l| l.name.eq_ignore_ascii_case(name)).peekable();
        same.peek()?;
        Some(if same.all(|l| !l.works && !l.public) { Found::Broken } else { Found::Works })
    }))
}

/// A shortcut to write: what it starts (a program by its path, or the app of a package), its
/// arguments, the folder it starts in and the file of its icon (the exe's first icon).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Spec {
    pub(crate) target: Target,
    pub(crate) args: String,
    pub(crate) dir: PathBuf,
    pub(crate) icon: PathBuf,
}

/// The shortcut of this copy: the Store copy its app (its icon comes with it); zip and installer
/// the exe; the test copy PowerShell on the `avvia.ps1` next to the exe, as its install script
/// makes it (`tools/test-copy/install-windows.ps1`), which keeps the log always on. `None` for
/// the Store copy when its AUMID could not be read.
pub(crate) fn spec(me: &Me, system_root: &Path) -> Option<Spec> {
    let dir = me.exe.parent()?.to_path_buf();
    let icon = me.exe.clone();
    Some(match me.copy {
        Copy::Store => Spec { target: Target::App(me.aumid.clone()?), args: String::new(), dir, icon },
        Copy::Zip | Copy::Installer => Spec { target: Target::Exe(me.exe.clone()), args: String::new(), dir, icon },
        Copy::Test => {
            let powershell = system_root.join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe");
            let args = format!(r#"-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "{}""#, dir.join("avvia.ps1").display());
            Spec { target: Target::Exe(powershell), args, dir, icon }
        }
    })
}

/// This copy, when it can have a desktop icon (`copy_kind`): read once.
pub(crate) fn me() -> Option<&'static Me> {
    static ME: OnceLock<Option<Me>> = OnceLock::new();
    ME.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        let copy = copy_kind(cfg!(windows), crate::update::packaged(), cfg!(feature = "test-copy"), &exe, Path::exists)?;
        let aumid = if copy == Copy::Store { shell::aumid() } else { None };
        Some(Me { copy, exe, aumid })
    })
    .as_ref()
}

/// Whether the first-start window asks (`asks`): this copy can have an icon and none starts it.
/// A desktop that cannot be read counts as without one: Yes then says why it failed.
pub(crate) fn ask_at_start() -> bool {
    // Debug only, for the screenshots of the question on Linux: `EYECANSEE_DESKTOP_QUESTION=1`.
    if cfg!(debug_assertions) && std::env::var_os("EYECANSEE_DESKTOP_QUESTION").is_some() {
        return true;
    }
    let Some(me) = me() else { return false };
    let present = shell::com(shell::links).is_ok_and(|links| decide(me, &links) == Outcome::There);
    asks(Some(me.copy), present)
}

/// What Yes of the question and Tools > Create desktop icon did.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Made {
    /// The icon with this name is on the user's desktop.
    Made(String),
    /// One that starts this copy was there already.
    There,
}

/// Makes the icon of this copy on the user's desktop, unless one starts it already. The error is
/// the system's, for the message.
pub(crate) fn create() -> Result<Made, String> {
    let me = me().ok_or("not a copy with a desktop icon")?;
    shell::com(|| {
        let place = match decide(me, &shell::links()?) {
            Outcome::There => return Ok(Made::There),
            Outcome::Write(place) => place,
        };
        let name = match &place {
            Place::New(n) | Place::Replace(n) => n.clone(),
        };
        let spec = spec(me, &shell::system_root()?).ok_or("the app of the package could not be read")?;
        let folder = shell::desktop(false)?;
        log!("desktop icon: {place:?} in {} -> {spec:?}", folder.display());
        shell::write(&folder.join(format!("{name}.lnk")), &spec)?;
        Ok(Made::Made(name))
    })
}

/// Takes away the icon `name` (without ".lnk") from the user's desktop, made a moment ago by
/// `create` (No after Yes in the first-start window): only while it still starts this copy, so
/// a shortcut put there meanwhile by something else is never deleted.
pub(crate) fn remove(name: &str) -> Result<(), String> {
    let me = me().ok_or("not a copy with a desktop icon")?;
    let name = name.to_string();
    shell::com(move || {
        let path = shell::desktop(false)?.join(format!("{name}.lnk"));
        if !starts_me(&shell::read(&path)?, me) {
            return Err(format!("{} no longer starts this copy", path.display()));
        }
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        log!("desktop icon: {} taken away", path.display());
        Ok(())
    })
}

/// Off Windows nothing has a desktop icon (`me` is `None`): these are never reached.
#[cfg(not(windows))]
mod shell {
    use super::{Link, Spec};
    use std::path::{Path, PathBuf};

    pub(super) fn com<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        f()
    }
    pub(super) fn aumid() -> Option<String> {
        None
    }
    pub(super) fn links() -> Result<Vec<Link>, String> {
        Ok(Vec::new())
    }
    pub(super) fn system_root() -> Result<PathBuf, String> {
        Err("Windows only".into())
    }
    pub(super) fn desktop(_public: bool) -> Result<PathBuf, String> {
        Err("Windows only".into())
    }
    pub(super) fn write(_path: &Path, _spec: &Spec) -> Result<(), String> {
        Err("Windows only".into())
    }
    pub(super) fn read(_path: &Path) -> Result<super::Target, String> {
        Err("Windows only".into())
    }
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
    fn link(name: &str, target: Target, works: bool, public: bool) -> Link {
        Link { name: name.into(), target, works, public }
    }

    /// A shortcut that starts this copy, on either desktop, means nothing to ask or make; the
    /// others decide the name.
    #[test]
    fn what_the_desktop_calls_for() {
        let me = Me { copy: Copy::Zip, exe: "C:/Tools/EyeCanSee/eyecansee.exe".into(), aumid: None };
        let mine = Target::Exe("c:/tools/eyecansee/eyecansee.exe".into());
        let other = Target::Exe("C:/Other/eyecansee.exe".into());
        assert_eq!(decide(&me, &[]), Outcome::Write(Place::New("EyeCanSee".into())));
        assert_eq!(decide(&me, &[link("Photos", mine.clone(), true, true)]), Outcome::There, "on the public desktop, any name");
        assert_eq!(decide(&me, &[link("EyeCanSee", other.clone(), false, false)]), Outcome::Write(Place::Replace("EyeCanSee".into())));
        assert_eq!(decide(&me, &[link("eyecansee", other.clone(), true, false)]), Outcome::Write(Place::New("EyeCanSee (2)".into())), "names without regard to case");
        // A broken one on the public desktop needs an administrator: its name is skipped.
        assert_eq!(decide(&me, &[link("EyeCanSee", other.clone(), false, true)]), Outcome::Write(Place::New("EyeCanSee (2)".into())));
        // One that could not be read (an icon dragged from Start read wrongly) is never replaced.
        assert_eq!(decide(&me, &[link("EyeCanSee", Target::Unread, works(&Target::Unread, |_| false), false)]), Outcome::Write(Place::New("EyeCanSee (2)".into())));
        let both = [link("EyeCanSee", other.clone(), false, false), link("EyeCanSee", other, true, true)];
        assert_eq!(decide(&me, &both), Outcome::Write(Place::New("EyeCanSee (2)".into())));
    }

    #[test]
    fn what_works() {
        let here = |p: &Path| p.ends_with("eyecansee.exe");
        assert!(works(&Target::Exe("C:/A/eyecansee.exe".into()), here));
        assert!(!works(&Target::Exe("C:/Gone/other.exe".into()), here));
        assert!(!works(&Target::Script("C:/Gone/avvia.ps1".into()), here));
        assert!(works(&Target::App("Maker.App_abc!App".into()), |_| false));
    }

    /// Each copy's shortcut, and read back it starts that copy.
    #[test]
    fn the_shortcut_of_each_copy() {
        let root = Path::new("C:/Windows");
        let zip = Me { copy: Copy::Zip, exe: "C:/Tools/EyeCanSee/eyecansee.exe".into(), aumid: None };
        let s = spec(&zip, root).unwrap();
        assert_eq!((s.target.clone(), s.args.as_str(), s.dir.as_path(), s.icon.as_path()), (Target::Exe(zip.exe.clone()), "", Path::new("C:/Tools/EyeCanSee"), zip.exe.as_path()));
        let installer = Me { copy: Copy::Installer, ..Me { copy: Copy::Zip, exe: zip.exe.clone(), aumid: None } };
        assert_eq!(spec(&installer, root).unwrap(), s);
        let store = Me { copy: Copy::Store, exe: "C:/Program Files/WindowsApps/x/eyecansee.exe".into(), aumid: Some("Maker.App_abc!App".into()) };
        assert_eq!(spec(&store, root).unwrap().target, Target::App("Maker.App_abc!App".into()));
        assert_eq!(spec(&Me { aumid: None, ..store }, root), None);
        let test = Me { copy: Copy::Test, exe: "C:/P/EyeCanSee-prova/eyecansee.exe".into(), aumid: None };
        let t = spec(&test, root).unwrap();
        assert!(matches!(&t.target, Target::Exe(p) if p.ends_with("powershell.exe")));
        assert_eq!(t.icon, test.exe, "the red icon of the test copy's exe");
        let Target::Exe(ps) = &t.target else { unreachable!() };
        for me in [&zip, &test] {
            let written = spec(me, root).unwrap();
            let program = match &written.target {
                Target::Exe(p) => p.clone(),
                _ => unreachable!(),
            };
            assert!(starts_me(&target_of(&program, &written.args), me), "{:?}", me.copy);
        }
        assert!(!starts_me(&target_of(ps, &t.args), &zip));
    }
}
