//! Updates from the GitHub releases: at startup (if enabled in Options) the latest release is
//! asked for in the background; a newer one is downloaded, checked against the SHA-256 GitHub
//! publishes for it, and installed when the user clicks "Install and restart".
//! Only copies that can replace themselves: the Windows installer and zip, the AppImage. The RPM
//! is updated by the system (`RPM`: dnf, or rpm-ostree on Fedora Atomic), the Store and Flatpak
//! copies by their own (`packaged`), development builds
//! (in `target`) never. The Store copy asks the Store whether it has a newer version and offers
//! its page (`Kind::Store`).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/IoSonoPiero/eyecansee/releases/latest";

/// minisign public keys whose signatures an update must carry (`tools/sign-release.sh` signs the
/// release files on the developer's PC; the private key never goes to GitHub). To rotate the key, one
/// release accepts both the old and the new one.
const PUBLIC_KEYS: &[&str] = &["RWRgiMFmEPRN5JASovMqT3NdY1Ui/mhebEzDTdsmKDiXno3K/9B4/biD"];

/// How this copy was installed.
#[derive(Debug, Clone)]
pub enum Kind {
    /// Windows, Inno Setup: the new installer runs in the same mode (per user or for all), with only
    /// its progress window (`/SILENT`): no questions, but on a slow PC it shows that something happens.
    Installer { all_users: bool },
    /// Windows zip: the files of the new zip replace the ones next to the exe.
    Zip,
    /// Linux AppImage at this path (`$APPIMAGE`): the new file takes its place.
    AppImage(PathBuf),
    /// Windows, from the Microsoft Store: the Store is asked for a newer version, and the button
    /// opens EyeCanSee's page there, where it is installed.
    #[cfg(windows)]
    Store,
}

/// EyeCanSee's page in the Microsoft Store app.
#[cfg(windows)]
const STORE_PAGE: &str = "ms-windows-store://pdp/?productid=9PK1598MXLBK";

/// Running from an MSIX package (the Microsoft Store): its folder is read-only and the Store
/// updates it.
#[cfg(windows)]
pub fn packaged() -> bool {
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;
    let mut len = 0;
    // SAFETY: a null buffer with length 0 only asks for the length (ERROR_INSUFFICIENT_BUFFER in a package).
    unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) != APPMODEL_ERROR_NO_PACKAGE }
}

/// Running as a Flatpak: flatpak updates it (`flatpak update`, GNOME Software, Discover).
#[cfg(not(windows))]
pub fn packaged() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

/// Built for the Fedora package: `eyecansee.spec` builds with the `rpm` feature, so the RPM of COPR
/// is known for sure. A test on the path (/usr/bin) would also take a copy built from the sources
/// and installed there by hand, which nothing updates.
pub const RPM: bool = cfg!(feature = "rpm");

/// Updated by the system, never by EyeCanSee: the Flatpak and the Fedora package. Their option
/// "Check for updates at startup" would do nothing and is hidden; the Store copy keeps it, as it
/// asks the Store.
pub fn by_system() -> bool {
    RPM || (!cfg!(windows) && packaged())
}

/// What Help > Check for updates says in a copy that EyeCanSee does not update (Fluent id).
pub fn not_updated_note() -> &'static str {
    // Fedora Atomic (Silverblue, Kinoite…) boots from an ostree deployment, which leaves this file:
    // the RPM is layered on it with rpm-ostree, and dnf cannot install there.
    let ostree = RPM && Path::new("/run/ostree-booted").exists();
    not_updated_id(!cfg!(windows) && packaged(), RPM, ostree)
}

fn not_updated_id(flatpak: bool, rpm: bool, ostree: bool) -> &'static str {
    match (flatpak, rpm, ostree) {
        (true, ..) => "update-flatpak",
        (false, true, true) => "update-rpm-ostree",
        (false, true, false) => "update-rpm",
        (false, false, _) => "update-unavailable",
    }
}

impl Kind {
    pub fn detect() -> Option<Kind> {
        // The private test copy is replaced by hand, never by a public release (`test_copy`); the
        // Fedora package by the system.
        if cfg!(feature = "test-copy") || RPM {
            return None;
        }
        // flatpak updates its own copies; the Microsoft Store too, but it is asked whether it has one.
        if packaged() {
            return Self::store();
        }
        if let Some(path) = std::env::var_os("APPIMAGE") {
            return Some(Kind::AppImage(path.into()));
        }
        if !cfg!(windows) {
            return None;
        }
        let exe = std::env::current_exe().ok()?;
        if exe.components().any(|c| c.as_os_str() == "target") {
            return None;
        }
        let dir = exe.parent()?;
        if dir.join("unins000.exe").exists() {
            let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
            let all_users = program_files.is_some_and(|p| exe.starts_with(p));
            return Some(Kind::Installer { all_users });
        }
        Some(Kind::Zip)
    }

    #[cfg(windows)]
    fn store() -> Option<Kind> {
        Some(Kind::Store)
    }

    #[cfg(not(windows))]
    fn store() -> Option<Kind> {
        None
    }

    /// End of the release file name for this kind.
    fn asset_suffix(&self) -> &'static str {
        match self {
            Kind::Installer { .. } => "-windows-x64-setup.exe",
            Kind::Zip => "-windows-x64.zip",
            Kind::AppImage(_) => "-x86_64.AppImage",
            #[cfg(windows)]
            Kind::Store => "",
        }
    }
}

/// A newer version downloaded and checked, waiting for "Install and restart".
#[derive(Debug, Clone)]
pub struct Ready {
    pub version: String,
    kind: Kind,
    file: PathBuf,
}

impl Ready {
    /// A newer version in the Microsoft Store: its button opens the Store page, nothing is installed here.
    pub fn in_store(&self) -> bool {
        #[cfg(windows)]
        return matches!(self.kind, Kind::Store);
        #[cfg(not(windows))]
        false
    }
}

/// "0.24.5" → (0, 24, 5); anything else (a suffix like "-test1") → `None`.
fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|p| p.parse().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

/// No byte for this long while a file arrives: the download stops with an error. The time runs
/// from one piece to the next, never for the whole file (a component of 60 MB on a slow line takes
/// minutes); a working connection, however slow, delivers something every few seconds. 60 s is
/// twice the time allowed for the headers; the words of `download-stalled` say "a minute".
const STALL: Duration = Duration::from_secs(60);

/// The largest update accepted: about five times the largest file of a release today (the Windows
/// zip, 53 MB in 0.76.2). Beyond it the download stops instead of filling the disk.
const MAX_UPDATE: u64 = 256 << 20;

pub(crate) fn agent() -> ureq::Agent {
    agent_with(STALL)
}

/// `agent` that gives up after `stall` without a byte (`STALL`; the tests use a short one).
pub(crate) fn agent_with(stall: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(stall))
        .user_agent(concat!("EyeCanSee/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Asks GitHub for the latest release; if newer than `current`, downloads its file for `kind`.
/// `Ok(None)` = already up to date.
pub fn check(kind: Kind, current: &str) -> Result<Option<Ready>, String> {
    #[cfg(windows)]
    if let Kind::Store = kind {
        return Ok(store_update()?.then(|| Ready { version: String::new(), kind, file: PathBuf::new() }));
    }
    let agent = agent();
    let text = agent.get(LATEST).call().and_then(|mut r| r.body_mut().read_to_string()).map_err(|e| e.to_string())?;
    let release: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let version = tag.trim_start_matches('v');
    match (parse(version), parse(current)) {
        (Some(new), Some(now)) if new > now => {}
        _ => return Ok(None),
    }
    let assets = release["assets"].as_array().map(Vec::as_slice).unwrap_or_default();
    let asset = assets
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(kind.asset_suffix())))
        .ok_or_else(|| format!("no {} in {tag}", kind.asset_suffix()))?;
    let (Some(name), Some(url)) = (asset["name"].as_str(), asset["browser_download_url"].as_str()) else {
        return Err(format!("incomplete asset in {tag}"));
    };
    // GitHub works the digest out itself when the file is uploaded.
    // It only catches a broken download: whoever can upload the file sets the digest too. The
    // signature is what proves the file comes from the developer.
    let sha = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).ok_or_else(|| format!("no SHA-256 for {name}"))?;
    let minisig = agent
        .get(signature_url(assets, name, tag)?)
        .call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .map_err(|e| e.to_string())?;
    let signed = |path: &Path| {
        let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        verify_signature(&mut f, &minisig, name, version, PUBLIC_KEYS)
    };
    let file = match &kind {
        // Next to the AppImage, so that putting it in place is a rename on the same disk.
        Kind::AppImage(path) => path.with_extension("AppImage.new"),
        _ => std::env::temp_dir().join(name),
    };
    // The size GitHub gives for the file, if any, and never beyond `MAX_UPDATE`.
    let max = asset["size"].as_u64().unwrap_or(MAX_UPDATE).min(MAX_UPDATE);
    fetch(&agent, url, &file, sha, max, signed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(Some(Ready { version: version.to_string(), kind, file }))
}

/// Whether the Microsoft Store has an update of this package. The Store does not say which version:
/// `StorePackageUpdate::Package` is the installed package, not the new one (seen with the real Store).
/// The Store answers only for a package it knows (the same name and publisher as in Partner Center).
#[cfg(windows)]
fn store_update() -> Result<bool, String> {
    use windows::Services::Store::StoreContext;
    let updates = StoreContext::GetDefault()
        .and_then(|c| c.GetAppAndOptionalStorePackageUpdatesAsync())
        .and_then(|op| op.join())
        .map_err(|e| format!("Microsoft Store: {e}"))?;
    Ok(updates.Size().map_err(|e| e.to_string())? > 0)
}

/// Opens EyeCanSee's page in the Microsoft Store app.
#[cfg(windows)]
pub fn open_store() -> Result<(), String> {
    // Through the handler Windows registered for the address, as a link: `explorer <uri>` opened
    // Documents instead. No console window; the address has no `&` for cmd.
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd").args(["/C", "start", "", STORE_PAGE]).creation_flags(CREATE_NO_WINDOW).spawn().map(drop).map_err(|e| e.to_string())
}

/// Download address of `<name>.minisig`: without it there is no update.
fn signature_url<'a>(assets: &'a [serde_json::Value], name: &str, tag: &str) -> Result<&'a str, String> {
    let sig = format!("{name}.minisig");
    assets
        .iter()
        .find(|a| a["name"].as_str() == Some(sig.as_str()))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or_else(|| format!("no signature ({sig}) in {tag}"))
}

/// Checks the minisign signature `minisig` of the file in `reader` with any of `keys`, then its
/// trusted comment (signed as well): it must name this very file and version, so that a genuine
/// signature of another file, or of an older release, cannot be passed off for this one.
fn verify_signature(reader: &mut impl Read, minisig: &str, name: &str, version: &str, keys: &[&str]) -> Result<(), String> {
    use minisign_verify::{PublicKey, Signature};
    let sig = Signature::decode(minisig).map_err(|e| format!("signature of {name}: {e}"))?;
    let keys = keys.iter().map(|k| PublicKey::from_base64(k)).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    // Only the key with the signature's key id starts a verifier.
    let mut verifier = keys
        .iter()
        .find_map(|k| k.verify_stream(&sig).ok())
        .ok_or_else(|| format!("{name} is not signed with an EyeCanSee key"))?;
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
    }
    verifier.finalize().map_err(|e| format!("signature of {name}: {e}"))?;
    let expected = format!("file:{name} version:{version}");
    if sig.trusted_comment() != expected {
        return Err(format!("signature of {name} is for \"{}\", expected \"{expected}\"", sig.trusted_comment()));
    }
    Ok(())
}

fn sha256_of(reader: &mut impl Read) -> Result<String, String> {
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect())
}

/// The error of a download that stalled (`STALL`): the Fluent id of its words (`error_detail`).
pub(crate) const STALLED: &str = "download-stalled";

/// What a failed read of a download says: `STALLED` for a stall, anything else as it is.
pub(crate) fn read_error(e: std::io::Error) -> String {
    let timeout = e.get_ref().and_then(|x| x.downcast_ref::<ureq::Error>()).is_some_and(|x| matches!(x, ureq::Error::Timeout(_)));
    if timeout || e.kind() == std::io::ErrorKind::TimedOut { STALLED.into() } else { e.to_string() }
}

/// The error of a download (update or component) as the windows show it: a stall in words.
pub(crate) fn error_detail(i18n: &crate::i18n::I18n, e: &str) -> String {
    if e == STALLED { i18n.t(STALLED) } else { e.to_string() }
}

/// One download of an update at a time: the check at start and Help > Check for updates wrote
/// the same `.part` (audit 2.9).
static FETCHING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `file`, downloaded unless it is there already from an earlier start and never installed: no
/// need to fetch it again, but it is checked like a new download. A second check while one is
/// downloading waits for it, then finds its file there.
fn fetch(agent: &ureq::Agent, url: &str, file: &Path, sha: &str, max: u64, signed: impl Fn(&Path) -> Result<(), String>) -> Result<(), String> {
    let _one = FETCHING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let cached = std::fs::File::open(file).ok().and_then(|mut f| sha256_of(&mut f).ok());
    if cached.as_deref() != Some(sha) || signed(file).is_err() {
        download(agent, url, file, sha, max, &signed)?;
    }
    Ok(())
}

/// Into a temporary name, renamed only once its SHA-256 matches and `signed` accepts it: a broken
/// or altered download never sits where it would be installed. More than `max` bytes is an error.
fn download(
    agent: &ureq::Agent,
    url: &str,
    file: &Path,
    sha: &str,
    max: u64,
    signed: impl Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let part = file.with_extension("part");
    let r = (|| {
        let mut body = agent.get(url).call().map_err(|e| e.to_string())?.into_body().into_reader();
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        let mut buf = vec![0; 1 << 16];
        let mut total = 0u64;
        loop {
            let n = body.read(&mut buf).map_err(read_error)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > max {
                return Err(format!("larger than expected: more than {max} bytes"));
            }
            ctx.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        let got: String = ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
        if got != sha {
            return Err(format!("SHA-256 mismatch: {got}, expected {sha}"));
        }
        drop(out);
        signed(&part)?;
        std::fs::rename(&part, file).map_err(|e| e.to_string())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

/// Starts a helper that waits for EyeCanSee to exit, installs the new version and starts it.
/// The caller then closes the window.
pub fn apply(ready: &Ready) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let pid = std::process::id();
    match &ready.kind {
        Kind::AppImage(path) => {
            // The AppImage runtime leaves its mount open in EyeCanSee (fd 1023, not close-on-exec):
            // passed on to the new AppImage, it kept the old one mounted and running until the new
            // one exited. Every inherited descriptor is marked close-on-exec before the helper starts.
            #[cfg(target_os = "linux")]
            // SAFETY: only sets a flag on this process's own descriptors, closes none.
            unsafe {
                libc::close_range(3, u32::MAX, libc::CLOSE_RANGE_CLOEXEC as i32);
            }
            std::process::Command::new("sh")
                .args(["-c", r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; mv -f "$2" "$3" && exec "$3""#, "sh"])
                .arg(pid.to_string())
                .arg(&ready.file)
                .arg(path)
                .spawn()
                .map(drop)
                .map_err(|e| e.to_string())
        }
        Kind::Installer { all_users } => {
            let mode = if *all_users { "/ALLUSERS" } else { "/CURRENTUSER" };
            powershell(&format!(
                "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; \
                 Start-Process -Wait -FilePath {setup} -ArgumentList '/SILENT','/SUPPRESSMSGBOXES','/NORESTART','{mode}'; \
                 Remove-Item -LiteralPath {setup}; Start-Process -FilePath {exe}",
                setup = quote(&ready.file),
                exe = quote(&exe),
            ))
        }
        Kind::Zip => {
            let dir = exe.parent().ok_or("exe without folder")?;
            let unpacked = ready.file.with_extension("unpacked");
            // The exe can stay locked for a moment after EyeCanSee has exited ("in use by another
            // process" when copying over it): renamed first, which Windows allows. On any error the
            // old one goes back, and EyeCanSee starts again in any case.
            powershell(&format!(
                "$ErrorActionPreference = 'Stop'; Wait-Process -Id {pid} -ErrorAction SilentlyContinue; \
                 try {{ Expand-Archive -LiteralPath {zip} -DestinationPath {unpacked} -Force; \
                 Move-Item -LiteralPath {exe} -Destination {old} -Force; \
                 Copy-Item -Path (Join-Path {unpacked} '*') -Destination {dir} -Recurse -Force; \
                 Remove-Item -LiteralPath {unpacked}, {zip} -Recurse -Force; \
                 Remove-Item -LiteralPath {old} -ErrorAction SilentlyContinue }} \
                 catch {{ if (-not (Test-Path -LiteralPath {exe})) {{ Move-Item -LiteralPath {old} -Destination {exe} }} }} \
                 finally {{ Start-Process -FilePath {exe} }}",
                zip = quote(&ready.file),
                unpacked = quote(&unpacked),
                dir = quote(dir),
                exe = quote(&exe),
                old = quote(&exe.with_extension("exe.old")),
            ))
        }
        // `Act::Update` opens the Store page instead (`in_store`).
        #[cfg(windows)]
        Kind::Store => Err("the Microsoft Store installs its own updates".into()),
    }
}

/// A PowerShell literal string: in single quotes, a quote is written twice.
/// EyeCanSee started again once this process has exited (the caller closes the window): after the
/// video engine is downloaded, which loaded only after a restart on Windows (2026-10-27). It opens
/// on the last folder, saved at closing.
pub fn restart() -> Result<(), String> {
    let pid = std::process::id();
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        powershell(&format!("Wait-Process -Id {pid} -ErrorAction SilentlyContinue; Start-Process -FilePath {}", quote(&exe)))
    }
    #[cfg(not(windows))]
    {
        // The AppImage itself, not the binary inside its mount (gone once this process exits).
        let exe = std::env::var_os("APPIMAGE").map(PathBuf::from).map_or_else(std::env::current_exe, Ok).map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        // SAFETY: only sets a flag on this process's own descriptors, closes none (see `apply`).
        unsafe {
            libc::close_range(3, u32::MAX, libc::CLOSE_RANGE_CLOEXEC as i32);
        }
        std::process::Command::new("sh")
            .args(["-c", r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; exec "$2""#, "sh"])
            .arg(pid.to_string())
            .arg(&exe)
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

fn powershell(script: &str) -> Result<(), String> {
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map(drop).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn versions_and_hash() {
        assert_eq!(parse("0.24.5"), Some((0, 24, 5)));
        assert!(parse("0.25.0") > parse("0.24.10"));
        assert_eq!(parse("0.21.0-test3"), None);
        assert_eq!(parse("1.2"), None);
        assert_eq!(
            sha256_of(&mut &b"abc"[..]).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(quote(Path::new(r"C:\Users\O'Neil\a.exe")), r"'C:\Users\O''Neil\a.exe'");
    }

    /// A PowerShell single-quoted string at the start of `s`, read as PowerShell reads it: it
    /// opens and closes with any of ' ‘ ’ ‚ ‛, and two of them in a row inside it stand for the first
    /// one (its tokenizer, `IsSingleQuote`). The text, and what follows the closing quote.
    fn ps_literal(s: &str) -> Option<(String, &str)> {
        let quote = |c: char| matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}');
        let mut chars = s.char_indices().peekable();
        if !chars.next().is_some_and(|(_, c)| quote(c)) {
            return None;
        }
        let mut text = String::new();
        while let Some((i, c)) = chars.next() {
            if !quote(c) {
                text.push(c);
            } else if chars.peek().is_some_and(|(_, n)| quote(*n)) {
                text.push(c);
                chars.next();
            } else {
                return Some((text, &s[i + c.len_utf8()..]));
            }
        }
        None
    }

    /// Audit 2.22: a folder with a typographic apostrophe (’, as Word and phones write it) in the
    /// path of the exe or of the download. PowerShell ends a single-quoted string at any of four
    /// quote characters besides ': the path was cut there, the rest of it read as commands, and
    /// the script of the update did not run (EyeCanSee closed, nothing installed, not started
    /// again). The path must come back whole from its literal, with nothing after it.
    #[test]
    fn quoted_paths_with_typographic_apostrophes() {
        for path in [r"C:\Users\O'Neil\a.exe", r"C:\Users\D’Angelo\AppData\Local\EyeCanSee\eyecansee.exe", r"D:\‘Foto’\‚x‛\''’’\a.zip"] {
            let quoted = quote(Path::new(path));
            assert_eq!(ps_literal(&quoted), Some((path.to_string(), "")), "{quoted}");
        }
    }

    /// Issue #69: the Fedora package says how the system updates it, rpm-ostree on Fedora Atomic.
    #[test]
    fn not_updated_notes() {
        assert_eq!(not_updated_id(true, false, false), "update-flatpak");
        assert_eq!(not_updated_id(false, true, false), "update-rpm");
        assert_eq!(not_updated_id(false, true, true), "update-rpm-ostree");
        assert_eq!(not_updated_id(false, false, false), "update-unavailable");
        // Only the spec builds the RPM: its build must keep the feature, or the option comes back.
        let spec = include_str!("../eyecansee.spec");
        assert!(spec.lines().any(|l| l.starts_with("cargo build") && l.contains("--features rpm")));
    }

    // Made with minisign 0.12 and two throwaway keys without password, A and B:
    // `minisign -S -s <key> -m <file> -t "<trusted comment>"` on a file holding FILE.
    const FILE: &[u8] = b"EyeCanSee test file\n";
    const KEY_A: &str = "RWQZas4plfKtEJBZ5hmmDaJ6b3pS5e9p43SogP/4U90uWV4qNdpucEOw";
    const KEY_B: &str = "RWTctaf0T4JxFe/ws2QoOpbFdzLpTzrNikfnyhBa8vU/iFxn266hPx/b";
    const NAME: &str = "EyeCanSee-9.9.9-x86_64.AppImage";
    const SIGNED_A: &str = "untrusted comment: signature from minisign secret key
RUQZas4plfKtEJWeS1kw6KBFBh/Gk49H9gWB0cwcsoEm0GUN8n+EEMyYU5OA7jncG838TUd94dwQsPkIPyFlV1IK295wzQwaVAs=
trusted comment: file:EyeCanSee-9.9.9-x86_64.AppImage version:9.9.9
50s9rYoeF2r919oMbb3E/9ju0fuk/aw6M/UFV03+YOmecrtJIkILMboq7P3ilOrRCcLlQ7Ukn4mIijcidD9xCA==
";
    const SIGNED_B: &str = "untrusted comment: signature from minisign secret key
RUTctaf0T4JxFXtypyobDMlxP4olZ6YITt7yMA2UJFIFwjO3uEbxawI0Xi8oHLkLBFkFWkjfwiRa13Yx+uG3HbYtGSGk55q6dAw=
trusted comment: file:EyeCanSee-9.9.9-x86_64.AppImage version:9.9.9
YV6pBbYT/oRZIHaULe6Gz4knBaQYQqklKWfFMQrVOnRVXEHxKA9I3g3KJbtZBjrT+wd7oLYOJj9LXy/68nymBg==
";
    const OLDER_VERSION: &str = "untrusted comment: signature from minisign secret key
RUQZas4plfKtEJWeS1kw6KBFBh/Gk49H9gWB0cwcsoEm0GUN8n+EEMyYU5OA7jncG838TUd94dwQsPkIPyFlV1IK295wzQwaVAs=
trusted comment: file:EyeCanSee-9.9.9-x86_64.AppImage version:9.9.8
+krccYs+9AUVsctzjLDzMpzHGgo7XbeySQWuwWKvvLgS1ecaIXto5dda6jbIpkQNmcIZgl2jPc0ShdoCsXHHAQ==
";
    const OTHER_FILE: &str = "untrusted comment: signature from minisign secret key
RUQZas4plfKtEJWeS1kw6KBFBh/Gk49H9gWB0cwcsoEm0GUN8n+EEMyYU5OA7jncG838TUd94dwQsPkIPyFlV1IK295wzQwaVAs=
trusted comment: file:EyeCanSee-9.9.9-windows-x64.zip version:9.9.9
ZNx9ootxLvzxqaW3Js8GQuuaRfw38173uezh9m7748ttQQ1wAqg5dxcDI9Q+3YopKvqk/hWD2HljErLOigFmAw==
";

    fn verify(file: &[u8], minisig: &str, keys: &[&str]) -> Result<(), String> {
        verify_signature(&mut &file[..], minisig, NAME, "9.9.9", keys)
    }

    #[test]
    fn signatures() {
        assert_eq!(verify(FILE, SIGNED_A, &[KEY_A]), Ok(()));
        // Any of the accepted keys (rotation).
        assert_eq!(verify(FILE, SIGNED_B, &[KEY_A, KEY_B]), Ok(()));
        assert!(verify(b"EyeCanSee test file!\n", SIGNED_A, &[KEY_A]).is_err());
        assert!(verify(FILE, SIGNED_B, &[KEY_A]).is_err());
        assert!(verify(FILE, OLDER_VERSION, &[KEY_A]).is_err());
        assert!(verify(FILE, OTHER_FILE, &[KEY_A]).is_err());
        assert!(verify(FILE, "", &[KEY_A]).is_err());
        // The trusted comment cannot be edited: it is signed with the file.
        assert!(verify(FILE, &SIGNED_A.replace("version:9.9.9", "version:9.9.8"), &[KEY_A]).is_err());
    }

    #[test]
    fn public_keys() {
        for k in PUBLIC_KEYS {
            minisign_verify::PublicKey::from_base64(k).unwrap();
        }
    }

    #[test]
    fn signature_asset() {
        let assets = serde_json::json!([
            { "name": NAME, "browser_download_url": "https://example.org/a" },
            { "name": "EyeCanSee-9.9.9-windows-x64.zip.minisig", "browser_download_url": "https://example.org/b" },
        ]);
        let assets = assets.as_array().unwrap();
        assert!(signature_url(assets, NAME, "v9.9.9").unwrap_err().contains(".AppImage.minisig"));
        assert_eq!(signature_url(assets, "EyeCanSee-9.9.9-windows-x64.zip", "v9.9.9"), Ok("https://example.org/b"));
    }

    /// A server on 127.0.0.1 for one download: it answers the first request with the headers of a
    /// file of `body.len()` bytes and the first `sent` of them, then stays silent with the
    /// connection open for `hold`. Its address.
    pub(crate) fn serve_once(body: Vec<u8>, sent: usize, hold: Duration) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file.bin", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = s.read(&mut [0; 4096]);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n", body.len());
            // The client may close first (an error found early): not this thread's business.
            let _ = s.write_all(head.as_bytes()).and_then(|()| s.write_all(&body[..sent.min(body.len())]));
            std::thread::sleep(hold);
        });
        url
    }

    /// As `serve_once`, the rest of the body sent once `gate` says so (or is dropped).
    fn serve_gated(body: Vec<u8>, sent: usize, gate: std::sync::mpsc::Receiver<()>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file.bin", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = s.read(&mut [0; 4096]);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n", body.len());
            let _ = s.write_all(head.as_bytes()).and_then(|()| s.write_all(&body[..sent]));
            let _ = gate.recv();
            let _ = s.write_all(&body[sent..]);
        });
        url
    }

    /// Audit 2.9 (a): two checks for updates at once (the one at start, Help > Check for updates)
    /// download the same file: the second one waits for the first, then finds its file there and
    /// checked. Before, it emptied the `.part` the first one was writing, and the first one failed.
    /// The first download is held half way by its server until the second one has ended, or for
    /// 3 s if it waits.
    #[test]
    fn audit_2_9_two_downloads_of_one_update() {
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let sha = sha256_of(&mut &body[..]).unwrap();
        let dir = scratch("twice");
        let file = dir.join("file.bin");
        let (open, gate) = std::sync::mpsc::channel();
        let urls = [serve_gated(body.clone(), 1000, gate), serve_once(body.clone(), body.len(), Duration::ZERO)];
        let fetching = |url: String| {
            let (file, sha, body) = (file.clone(), sha.clone(), body.clone());
            let whole = move |p: &Path| (std::fs::read(p).ok().as_deref() == Some(&body[..])).then_some(()).ok_or_else(|| "not the file".to_string());
            std::thread::spawn(move || fetch(&agent_with(Duration::from_secs(20)), &url, &file, &sha, MAX_UPDATE, whole))
        };
        let [first_url, second_url] = urls;
        let first = fetching(first_url);
        let part = file.with_extension("part");
        let started = std::time::Instant::now();
        while !std::fs::metadata(&part).is_ok_and(|m| m.len() > 0) {
            assert!(started.elapsed() < Duration::from_secs(20), "the first download did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        let second = fetching(second_url);
        let waited = std::time::Instant::now();
        while !second.is_finished() && waited.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
        }
        open.send(()).unwrap();
        let (first, second) = (first.join().unwrap(), second.join().unwrap());
        let got = std::fs::read(&file).ok();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((first, second), (Ok(()), Ok(())));
        assert!(got.as_deref() == Some(&body[..]), "the file is not whole");
    }

    /// A folder of its own for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Audit 2.15 (a): a download whose body stops arriving, with the connection left open (a
    /// network that drops without closing it), ends with an error once no byte has come for the
    /// stall time (`STALL`, here 1 s). The server sends the headers of a file of 1 MB, 1000
    /// bytes of it, then nothing more.
    #[test]
    fn audit_2_15_stalled_body() {
        let url = serve_once(vec![7; 1_000_000], 1000, Duration::from_secs(3600));
        let dir = scratch("stalled");
        let file = dir.join("file.bin");
        let (tx, rx) = std::sync::mpsc::channel();
        let (to, started) = (file.clone(), std::time::Instant::now());
        std::thread::spawn(move || tx.send(download(&agent_with(Duration::from_secs(1)), &url, &to, "00", MAX_UPDATE, |_| Ok(()))));
        let r = rx.recv_timeout(Duration::from_secs(10));
        let elapsed = started.elapsed();
        let left = file.with_extension("part").exists() || file.exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(r, Ok(Err(crate::update::STALLED.into())), "after {elapsed:.1?}");
        assert!(elapsed < Duration::from_secs(5), "{elapsed:.1?}");
        assert!(!left, "a stalled download left a file");
    }

    /// A download larger than `max` stops there, and nothing is left; one within it is put in place.
    #[test]
    fn download_size_ceiling() {
        let body = vec![5u8; 3000];
        let sha = sha256_of(&mut &body[..]).unwrap();
        let dir = scratch("ceiling");
        let file = dir.join("file.bin");
        let url = serve_once(body.clone(), body.len(), Duration::ZERO);
        let r = download(&agent_with(Duration::from_secs(5)), &url, &file, &sha, 1000, |_| Ok(()));
        let left = file.with_extension("part").exists() || file.exists();
        let url = serve_once(body.clone(), body.len(), Duration::ZERO);
        let ok = download(&agent_with(Duration::from_secs(5)), &url, &file, &sha, 3000, |_| Ok(()));
        let got = std::fs::read(&file).ok();
        let part = file.with_extension("part").exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(r, Err("larger than expected: more than 1000 bytes".into()));
        assert!(!left, "a download beyond the ceiling left a file");
        assert_eq!(ok, Ok(()));
        assert_eq!(got.as_deref(), Some(&body[..]));
        assert!(!part);
    }

    /// `cargo test update_live -- --ignored --nocapture`: the whole check against the real latest
    /// release, as an AppImage would do it (download, size, SHA-256, signature), in a temporary folder.
    #[test]
    #[ignore]
    fn update_live() {
        let dir = scratch("live");
        let r = check(Kind::AppImage(dir.join("x.AppImage")), "0.0.1");
        let got = std::fs::metadata(dir.join("x.AppImage.new")).map(|m| m.len());
        let left = std::fs::read_dir(&dir).unwrap().count();
        let _ = std::fs::remove_dir_all(&dir);
        println!("{r:?}, {got:?} bytes");
        assert!(r.unwrap().is_some());
        assert_eq!(left, 1);
    }
}
