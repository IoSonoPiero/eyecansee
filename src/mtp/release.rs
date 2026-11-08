//! Opening a phone the desktop holds (gvfs on GNOME, kiod on KDE), also from the Flatpak: moved out
//! of mtp.rs as it was, to keep that file under its size.

use super::*;

/// On Windows the phone belongs to the WPD driver, which knows it by a serial different from the
/// USB one (POCO X7 Pro: `open_by_serial` gives `NoDevice` from WPD, then `Unsupported` from USB).
/// It is opened by USB location: mtp-rs matches it to the WPD device by VID/PID.
#[cfg(windows)]
pub(super) async fn open(serial: &str) -> Result<MtpDevice, Error> {
    let found = MtpDevice::list_devices().map_err(error)?.into_iter().find(|d| d.serial_number.as_deref() == Some(serial));
    match found {
        Some(d) => MtpDevice::open_by_location(d.location_id).await.map_err(error),
        None => Err(error(mtp_rs::Error::NoDevice)),
    }
}

/// Set when a phone was taken from gvfs (`release_from_desktop`): the UI says so once
/// (`taken_from_gvfs`), as the file manager no longer shows it.
static FROM_GVFS: AtomicBool = AtomicBool::new(false);

pub fn taken_from_gvfs() -> bool {
    FROM_GVFS.swap(false, Ordering::Relaxed)
}

/// KDE's kiod may be terminated only after the user said yes (`allow_kiod`, "error-device-kde"):
/// it also forgets the passwords of network folders.
static KIOD_ALLOWED: AtomicBool = AtomicBool::new(false);

pub fn allow_kiod() {
    KIOD_ALLOWED.store(true, Ordering::Relaxed);
}

#[cfg(not(windows))]
pub(super) async fn open(serial: &str) -> Result<MtpDevice, Error> {
    match MtpDevice::open_by_serial(serial).await {
        Err(e) if e.is_exclusive_access() => match release_from_desktop(serial) {
            Release::Done => {
                log!("phone {serial} released from the desktop");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                MtpDevice::open_by_serial(serial).await.map_err(|e| match e.is_exclusive_access() && flatpak_kde() {
                    // kiod6 quit, but KDE took the phone back (Dolphin open on it): the user is asked again.
                    true => {
                        KIOD_ALLOWED.store(false, Ordering::Relaxed);
                        ("error-device-kde-back", e.to_string())
                    }
                    false => error(e),
                })
            }
            Release::AskKiod => Err(("error-device-kde", e.to_string())),
            Release::No => Err(error(e)),
        },
        r => r.map_err(error),
    }
}

#[cfg(not(windows))]
enum Release {
    Done,
    /// Only kiod holds it, and the user has not allowed terminating it yet.
    AskKiod,
    No,
}

/// GNOME: gvfs keeps the phone open and the direct connection finds it busy. Terminating the gvfs
/// process serving it frees it and the phone stays in "File transfer" (verified 2026-10-21;
/// `gio mount -u` instead put it back to "charging only"). gvfs does not take it back by itself:
/// a click in Nautilus or the next connection remounts it. The direct connection is much faster
/// in large folders (Pictures/9GAG: thumbnails in ~20 s instead of over 5 minutes).
/// KDE: kiod6 holds it through its kmtpd module, which D-Bus cannot unload; terminated, kiod6 comes
/// back on demand without kmtpd (verified on Kubuntu 26.04, 2026-10-25). It also forgets the
/// passwords it kept for network folders. Plasma 5: kiod5, the same (Kubuntu 24.04, 2026-10-25;
/// there the phone went back to "charging only" and opened once the user chose "File transfer").
#[cfg(not(windows))]
fn release_from_desktop(serial: &str) -> Release {
    let (mut released, mut ask) = (false, false);
    for (pid, comm) in holders(serial) {
        if comm.starts_with("kiod") && !KIOD_ALLOWED.load(Ordering::Relaxed) {
            ask = true;
            continue;
        }
        let killed = kill(&pid);
        if killed && comm.starts_with("gvfsd") {
            FROM_GVFS.store(true, Ordering::Relaxed);
        }
        released |= killed;
    }
    // Flatpak: the host's processes are out of sight, gvfs is asked over D-Bus instead.
    if !released && !ask && std::env::var_os("FLATPAK_ID").is_some() {
        released = gvfs_unmount(serial);
        FROM_GVFS.fetch_or(released, Ordering::Relaxed);
        // KDE: kiod6 is out of sight too; after the user's yes it is asked to quit over D-Bus.
        if !released && flatpak_kde() {
            match KIOD_ALLOWED.load(Ordering::Relaxed) {
                true => released = kiod_quit(),
                false => ask = true,
            }
        }
    }
    match (released, ask) {
        (true, _) => Release::Done,
        (false, true) => Release::AskKiod,
        _ => Release::No,
    }
}

/// KDE: when Dolphin asks for the phone while EyeCanSee has it, kiod starts again, cannot open it
/// ("device is busy… Could not open MTP device") and never tries again: Dolphin then says the
/// device "does not exist", also after EyeCanSee lets it go (Plasma 5, 2026-10-25). Called as
/// EyeCanSee lets the phone go: terminated, kiod starts again when Dolphin asks, and opens it.
/// Only once the user has allowed terminating kiod (it forgets network passwords).
#[cfg(not(windows))]
pub(super) fn drop_stale_kiod(serial: &str) {
    if !KIOD_ALLOWED.load(Ordering::Relaxed) {
        return;
    }
    for (pid, comm) in holders(serial) {
        if comm.starts_with("kiod") && kill(&pid) {
            log!("stale {comm} ({pid:?}) terminated for phone {serial}");
        }
    }
    // Flatpak: kiod6 is out of sight, so it is asked to quit whether it is stale or not (seen on
    // Plasma 6.8: Dolphin said "udi=… does not exist" after EyeCanSee let the phone go).
    if flatpak_kde() {
        kiod_quit();
    }
}

#[cfg(windows)]
pub(super) fn drop_stale_kiod(_: &str) {}

/// Desktop processes (gvfs, kiod) with the phone's USB file open: (pid, name).
#[cfg(not(windows))]
fn holders(serial: &str) -> Vec<(std::ffi::OsString, String)> {
    use std::fs::{read_dir, read_link, read_to_string};
    // The phone's USB file (/dev/bus/usb/001/006), from the serial.
    let Some(node) = read_dir("/sys/bus/usb/devices").into_iter().flatten().flatten().find_map(|d| {
        (read_to_string(d.path().join("serial")).ok()?.trim() == serial).then_some(())?;
        let uevent = read_to_string(d.path().join("uevent")).ok()?;
        uevent.lines().find_map(|l| l.strip_prefix("DEVNAME=")).map(|n| format!("/dev/{n}"))
    }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for p in read_dir("/proc").into_iter().flatten().flatten() {
        let comm = read_to_string(p.path().join("comm")).unwrap_or_default().trim().to_string();
        if !matches!(comm.as_str(), "gvfsd-mtp" | "gvfsd-gphoto2" | "kiod6" | "kiod5") {
            continue;
        }
        let holds = read_dir(p.path().join("fd"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|f| read_link(f.path()).is_ok_and(|l| l.to_string_lossy() == node));
        if holds {
            found.push((p.file_name(), comm));
        }
    }
    found
}

/// Unmounts the phone from gvfs over D-Bus (`--talk-name=org.gtk.vfs.*`), like Nautilus's Eject: a
/// Samsung stays in "File transfer" (2026-10-26), a POCO X7 Pro went back to "charging only"
/// (2026-10-21), so only once per phone: after that EyeCanSee uses the gvfs mount.
#[cfg(not(windows))]
fn gvfs_unmount(serial: &str) -> bool {
    static DONE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let mut done = DONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if done.iter().any(|s| s == serial) {
        return false;
    }
    let gdbus = |args: &[&str]| std::process::Command::new("gdbus").arg("call").arg("--session").args(args).output().ok().filter(|o| o.status.success());
    let tracker = ["--dest", "org.gtk.vfs.Daemon", "--object-path", "/org/gtk/vfs/mounttracker"];
    let Some(list) = gdbus(&[&tracker[..], &["--method", "org.gtk.vfs.MountTracker.ListMounts2", "false"]].concat()) else { return false };
    let Some((dest, path)) = gvfs_mount(&String::from_utf8_lossy(&list.stdout), serial) else { return false };
    done.push(serial.to_string());
    let unmount = ["--dest", &dest, "--object-path", &path, "--method", "org.gtk.vfs.Mount.Unmount", "", "/", "0"];
    gdbus(&unmount).is_some()
}

/// In the reply of gvfs's ListMounts2, the D-Bus name and object of the phone's mount
/// (`mtp:host=<maker>_<model>_<serial>`). ponytail: a display name with a `'` (double quotes in
/// the reply) shifts the fields and the mount is not found: EyeCanSee uses gvfs then.
#[cfg(not(windows))]
fn gvfs_mount(list: &str, serial: &str) -> Option<(String, String)> {
    list.split("('").skip(1).find_map(|m| {
        let f: Vec<&str> = m.split('\'').collect();
        let ours = f.get(6)?.starts_with("mtp:host=") && f[6].ends_with(&format!("_{serial}"));
        (ours && f[2].starts_with("/org/gtk/vfs/mount/")).then(|| (f[0].to_string(), f[2].to_string()))
    })
}

#[cfg(not(windows))]
fn flatpak_kde() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() && std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.contains("KDE"))
}

/// Flatpak on KDE (`--talk-name=org.kde.kiod6`): kiod6 asked to quit, as `kquitapp6 kiod6` does.
/// It lets the phone go; KDE starts it again when Dolphin needs it (tried on Plasma 6.8, 2026-11-08).
#[cfg(not(windows))]
fn kiod_quit() -> bool {
    let quit = ["call", "--session", "--dest", "org.kde.kiod6", "--object-path", "/MainApplication", "--method", "org.qtproject.Qt.QCoreApplication.quit"];
    let done = std::process::Command::new("gdbus").args(quit).output().is_ok_and(|o| o.status.success());
    log!("kiod6 asked to quit over D-Bus: {done}");
    done
}

#[cfg(not(windows))]
fn kill(pid: &std::ffi::OsStr) -> bool {
    std::process::Command::new("kill").arg(pid).status().is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn gvfs_mount_of_phone() {
        let list = "([(':1.919', objectpath '/org/gtk/vfs/mount/1', '', 'http:uri=https%3A%2F%2F127.0.0.1%3A8384', '', '', '', '', false, b'', (b'/', {'type': <b'http'>}), b''), (':1.1546', '/org/gtk/vfs/mount/1', 'mtp', 'mtp:host=SAMSUNG_SAMSUNG_Android_R52W80FSD5D', '', '. GThemedIcon phone', '', '', true, b'/run/user/1000/gvfs/mtp:host=SAMSUNG_SAMSUNG_Android_R52W80FSD5D', (b'/', {'host': <b'SAMSUNG_SAMSUNG_Android_R52W80FSD5D'>, 'type': <b'mtp'>}), b'')],)";
        assert_eq!(gvfs_mount(list, "R52W80FSD5D"), Some((":1.1546".into(), "/org/gtk/vfs/mount/1".into())));
        assert_eq!(gvfs_mount(list, "80FSD5D"), None);
        assert_eq!(gvfs_mount("([],)", "R52W80FSD5D"), None);
    }
}
