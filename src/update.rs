//! Updates from the GitHub releases: at startup (if enabled in Options) the latest release is
//! asked for in the background; a newer one is downloaded, checked against the SHA-256 GitHub
//! publishes for it, and installed when the user clicks "Install and restart".
//! Only copies that can replace themselves: the Windows installer and zip, the AppImage. The RPM
//! is updated by dnf, development builds (in `target`) never.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/IoSonoPiero/eyecansee/releases/latest";

/// How this copy was installed.
#[derive(Debug, Clone)]
pub enum Kind {
    /// Windows, Inno Setup: the new installer runs silently in the same mode (per user or for all).
    Installer { all_users: bool },
    /// Windows zip: the files of the new zip replace the ones next to the exe.
    Zip,
    /// Linux AppImage at this path (`$APPIMAGE`): the new file takes its place.
    AppImage(PathBuf),
}

impl Kind {
    pub fn detect() -> Option<Kind> {
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

    /// End of the release file name for this kind.
    fn asset_suffix(&self) -> &'static str {
        match self {
            Kind::Installer { .. } => "-windows-x64-setup.exe",
            Kind::Zip => "-windows-x64.zip",
            Kind::AppImage(_) => "-x86_64.AppImage",
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

/// "0.24.5" → (0, 24, 5); anything else (a suffix like "-test1") → `None`.
fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|p| p.parse().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .user_agent(concat!("EyeCanSee/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Asks GitHub for the latest release; if newer than `current`, downloads its file for `kind`.
/// `Ok(None)` = already up to date.
pub fn check(kind: Kind, current: &str) -> Result<Option<Ready>, String> {
    let agent = agent();
    let text = agent.get(LATEST).call().and_then(|mut r| r.body_mut().read_to_string()).map_err(|e| e.to_string())?;
    let release: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let version = tag.trim_start_matches('v');
    match (parse(version), parse(current)) {
        (Some(new), Some(now)) if new > now => {}
        _ => return Ok(None),
    }
    let asset = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(kind.asset_suffix()))))
        .ok_or_else(|| format!("no {} in {tag}", kind.asset_suffix()))?;
    let (Some(name), Some(url)) = (asset["name"].as_str(), asset["browser_download_url"].as_str()) else {
        return Err(format!("incomplete asset in {tag}"));
    };
    // GitHub works the digest out itself when the file is uploaded.
    let sha = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).ok_or_else(|| format!("no SHA-256 for {name}"))?;
    let file = match &kind {
        // Next to the AppImage, so that putting it in place is a rename on the same disk.
        Kind::AppImage(path) => path.with_extension("AppImage.new"),
        _ => std::env::temp_dir().join(name),
    };
    // Downloaded at an earlier start and never installed: no need to fetch it again.
    let cached = std::fs::File::open(&file).ok().and_then(|mut f| sha256_of(&mut f).ok());
    if cached.as_deref() != Some(sha) {
        download(&agent, url, &file, sha)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(Some(Ready { version: version.to_string(), kind, file }))
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

/// Into a temporary name, renamed only once its SHA-256 matches: a broken or altered download
/// never sits where it would be installed.
fn download(agent: &ureq::Agent, url: &str, file: &Path, sha: &str) -> Result<(), String> {
    let part = file.with_extension("part");
    let r = (|| {
        let mut body = agent.get(url).call().map_err(|e| e.to_string())?.into_body().into_reader();
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        let mut buf = vec![0; 1 << 16];
        loop {
            let n = body.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            ctx.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        let got: String = ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
        if got != sha {
            return Err(format!("SHA-256 mismatch: {got}, expected {sha}"));
        }
        drop(out);
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
                 Start-Process -Wait -FilePath {setup} -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','{mode}'; \
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
    }
}

/// A PowerShell literal string: in single quotes, a quote is written twice.
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
mod tests {
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
}
