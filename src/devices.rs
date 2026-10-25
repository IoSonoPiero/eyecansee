//! Connected external devices: disks, USB sticks and SD cards, phones and cameras.
//! On Linux the system usually mounts them (udisks and gvfs): here we look at where it puts them
//! and browse them as local folders. Phones are opened directly over MTP (`mtp.rs`), even when
//! gvfs has mounted them; the gvfs mount remains for iPhones and as a fallback.
//! On Windows: drives from their letters (`drives`), phones via WPD.
//! Measured on 2026-10-20 on a phone in PTP mode via gvfs: listing a folder 0.2-0.6 s,
//! reading ~29 MB/s.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

use eframe::egui::Context;
use percent_encoding::percent_decode_str;

use crate::loader::Msg;

#[derive(Clone, PartialEq, Debug)]
pub struct Device {
    pub name: String,
    /// Absolute path ending in `/`, or `mtp://<serial>/` for the direct connection.
    pub path: String,
    pub phone: bool,
    /// USB serial number, to recognise the same phone when mounted and when connected directly.
    pub serial: Option<String>,
    /// Unmounted external partition (`/dev/sda1`): `path` is empty, a click mounts it.
    pub block: Option<String>,
    /// Internal Windows drive (`C:\`): goes under "This computer", not among the devices.
    pub internal: bool,
}

/// gvfs prefixes of phones and cameras; network shares and the like stay out.
const PHONE_SCHEMES: &[&str] = &["mtp", "gphoto2", "afc"];

/// "gphoto2:host=Xiaomi_POCO_X7_Pro_ABCD1234EFGH5678" → ("Xiaomi POCO X7 Pro", serial).
fn gvfs_name(entry: &str) -> Option<(String, Option<String>)> {
    let (scheme, rest) = entry.split_once(':')?;
    if !PHONE_SCHEMES.contains(&scheme) {
        return None;
    }
    let host = rest.strip_prefix("host=").unwrap_or(rest);
    let host = percent_decode_str(host).decode_utf8_lossy();
    let mut words: Vec<&str> = host.split('_').filter(|w| !w.is_empty()).collect();
    let serial = |w: &str| w.len() >= 8 && w.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let serial = if words.len() > 1 && words.last().is_some_and(|w| serial(w)) {
        words.pop().map(str::to_string)
    } else {
        None
    };
    Some((words.join(" "), serial))
}

fn entries(dir: &Path) -> Vec<(String, String)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), format!("{}/", e.path().to_string_lossy())))
        .collect()
}

/// Mounted external disks, wherever they are: /run/media or /media (automatic mounting) but also
/// /mnt from fstab (real case: ext4 partition of a USB SSD in /mnt/Linux, 2026-10-20). What counts
/// is the device, not the mount point: external if it sits on USB in /sys, or an SD card.
fn external_disks() -> Vec<Device> {
    let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else { return Vec::new() };
    // Partition labels: /dev/disk/by-label/<label> -> ../../sda1
    let labels: std::collections::HashMap<String, String> = std::fs::read_dir("/dev/disk/by-label")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let target = std::fs::read_link(e.path()).ok()?;
            let dev = target.file_name()?.to_string_lossy().into_owned();
            Some((dev, unescape_udev(&e.file_name().to_string_lossy())))
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut disks = Vec::new();
    for line in mounts.lines() {
        let mut fields = line.split(' ');
        let (Some(dev), Some(mount)) = (fields.next(), fields.next()) else { continue };
        let Some(dev) = dev.strip_prefix("/dev/") else { continue };
        if !is_external(dev) || !seen.insert(dev.to_string()) {
            continue;
        }
        let mount = unescape_mount(mount);
        let name = labels.get(dev).cloned().unwrap_or_else(|| {
            Path::new(&mount).file_name().map_or(dev.to_string(), |n| n.to_string_lossy().into_owned())
        });
        let path = format!("{}/", mount.trim_end_matches('/'));
        disks.push(Device { name, path, phone: false, serial: None, block: None, internal: false });
    }
    // External partitions with a file system that are not mounted: for example a partition with
    // a line in /etc/fstab, which the desktop does not mount by itself when plugged in. Type and
    // label from the udev database; encrypted (crypto) and swap stay out.
    for e in std::fs::read_dir("/sys/class/block").into_iter().flatten().flatten() {
        let dev = e.file_name().to_string_lossy().into_owned();
        if seen.contains(&dev) || !is_external(&dev) {
            continue;
        }
        let Ok(numbers) = std::fs::read_to_string(e.path().join("dev")) else { continue };
        let Ok(udev) = std::fs::read_to_string(format!("/run/udev/data/b{}", numbers.trim())) else { continue };
        let prop = |key: &str| udev.lines().find_map(|l| l.strip_prefix(&format!("E:{key}=")).map(str::to_string));
        if prop("ID_FS_USAGE").as_deref() != Some("filesystem") {
            continue;
        }
        let name = prop("ID_FS_LABEL_ENC").map(|l| unescape_udev(&l)).filter(|l| !l.is_empty()).unwrap_or_else(|| dev.clone());
        let block = Some(format!("/dev/{dev}"));
        disks.push(Device { name, path: String::new(), phone: false, serial: None, block, internal: false });
    }
    disks
}

/// Mounts a partition with udisks, like the file manager (honours /etc/fstab; if needed, the
/// system asks for the password with its own dialog). Replies with the mount path.
pub fn mount(block: String, tx: Sender<Msg>, ctx: Context) {
    let _ = std::thread::Builder::new().name("mount".into()).spawn(move || {
        let r = match std::process::Command::new("udisksctl").args(["mount", "-b", &block]).output() {
            // "Mounted /dev/sda1 at /mnt/Linux" (sometimes with a trailing full stop)
            Ok(o) if o.status.success() => {
                let out = String::from_utf8_lossy(&o.stdout);
                out.trim()
                    .strip_prefix(&format!("Mounted {block} at "))
                    .map(|p| p.trim_end_matches('.').to_string())
                    .ok_or_else(|| out.trim().to_string())
            }
            Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Err(format!("udisksctl: {e}")),
        };
        let _ = tx.send(Msg::Mounted(r));
        ctx.request_repaint();
    });
}

fn is_external(dev: &str) -> bool {
    let sys = std::fs::canonicalize(format!("/sys/class/block/{dev}"));
    dev.starts_with("mmcblk") || sys.is_ok_and(|p| p.to_string_lossy().contains("/usb"))
}

/// A device in `old` but no longer in `new` (by path) that holds the folder `current` (absolute,
/// as the address bar shows it): unplugged, or unmounted, with one of its folders open.
pub fn gone_under(old: &[Device], new: &[Device], current: &str) -> bool {
    old.iter()
        .filter(|d| !d.path.is_empty() && !new.iter().any(|n| n.path == d.path))
        .any(|d| current.starts_with(&d.path) || current == d.path.trim_end_matches(['/', '\\']))
}

/// Does the drive of this path have a Recycle Bin? On Windows only fixed disks do: on USB sticks,
/// cards and network drives "to the Recycle Bin" deletes for good, without a word (the `trash`
/// crate asks with `FOF_NO_UI`, which also hides Windows' warning; NTFS stick, 2026-10-25).
#[cfg(windows)]
pub fn has_recycle_bin(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_FIXED: u32 = 3;
    // "D:\" for D:\Photos\a.jpg, "\\server\share\" for a network path.
    let Some(root) = path.ancestors().last() else { return false };
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a NUL-terminated wide string that outlives the call.
    unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_FIXED }
}

#[cfg(not(windows))]
pub fn has_recycle_bin(_: &Path) -> bool {
    true
}

/// Is this absolute path on an NTFS disk? (Linux: `ntfs3`, or `fuseblk`, which is ntfs-3g on the
/// usual desktops [Inference: exFAT through FUSE would also match].) There Nautilus does not
/// restore from the trash, not even what GNOME itself put there (2026-10-22): the confirmation says so.
pub fn on_ntfs(path: &str) -> bool {
    std::fs::read_to_string("/proc/self/mounts").is_ok_and(|m| ntfs_in(&m, path))
}

fn ntfs_in(mounts: &str, path: &str) -> bool {
    let inside = |mount: &str| path == mount || path.starts_with(&format!("{}/", mount.trim_end_matches('/')));
    mounts
        .lines()
        .filter_map(|l| {
            let mut f = l.split(' ');
            let (_, mount, kind) = (f.next()?, unescape_mount(f.next()?), f.next()?);
            inside(&mount).then_some((mount.len(), kind))
        })
        .max_by_key(|(len, _)| *len)
        .is_some_and(|(_, kind)| matches!(kind, "fuseblk" | "ntfs" | "ntfs3"))
}

/// /proc/self/mounts writes spaces and the like in octal: "Sea\040photos" → "Sea photos".
fn unescape_mount(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let octal = b.get(i + 1..i + 4).and_then(|d| u8::from_str_radix(std::str::from_utf8(d).ok()?, 8).ok());
        match (b[i], octal) {
            (b'\\', Some(c)) => {
                out.push(c);
                i += 4;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// udev writes special characters in labels as "\x20".
fn unescape_udev(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'\\' && b.get(i + 1) == Some(&b'x'))
            .then(|| b.get(i + 2..i + 4).and_then(|d| u8::from_str_radix(std::str::from_utf8(d).ok()?, 16).ok()))
            .flatten();
        match hex {
            Some(c) => {
                out.push(c);
                i += 4;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Windows drives with a readable file system. External: removable (USB sticks, cards) and fixed
/// disks on USB or SD, like USB SSDs that Windows considers fixed. Drives without media (empty
/// card reader) do not answer `GetVolumeInformationW` and stay out.
// ponytail: no network drives (a server that is off would block the scan) and no CD/DVD
#[cfg(windows)]
fn drives() -> Vec<Device> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BusTypeMmc, BusTypeSd, BusTypeUsb, CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW,
        GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
        StorageDeviceProperty,
    };
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_RAMDISK: u32 = 6;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();

    // Bus of the disk holding the drive; opened without access rights, enough for the query.
    let bus = |letter: char| unsafe {
        let handle = CreateFileW(
            wide(&format!(r"\\.\{letter}:")).as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut desc: STORAGE_DEVICE_DESCRIPTOR = std::mem::zeroed();
        let mut returned = 0;
        let ok = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&raw const query).cast(),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            (&raw mut desc).cast(),
            size_of::<STORAGE_DEVICE_DESCRIPTOR>() as u32,
            &mut returned,
            std::ptr::null_mut(),
        );
        CloseHandle(handle);
        (ok != 0).then_some(desc.BusType)
    };

    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new();
    for (i, letter) in ('A'..='Z').enumerate() {
        if mask & (1 << i) == 0 {
            continue;
        }
        let root = format!(r"{letter}:\");
        let kind = unsafe { GetDriveTypeW(wide(&root).as_ptr()) };
        if ![DRIVE_REMOVABLE, DRIVE_FIXED, DRIVE_RAMDISK].contains(&kind) {
            continue;
        }
        let mut label = [0u16; 261];
        let ok = unsafe {
            GetVolumeInformationW(
                wide(&root).as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            continue;
        }
        let len = label.iter().position(|&c| c == 0).unwrap_or(label.len());
        let label = String::from_utf16_lossy(&label[..len]);
        // Like File Explorer: "Backup (D:)", just "D:" without a label.
        let name = if label.is_empty() { format!("{letter}:") } else { format!("{label} ({letter}:)") };
        let external = kind == DRIVE_REMOVABLE
            || (kind == DRIVE_FIXED && bus(letter).is_some_and(|b| [BusTypeUsb, BusTypeSd, BusTypeMmc].contains(&b)));
        drives.push(Device { name, path: root, phone: false, serial: None, block: None, internal: !external });
    }
    drives
}

/// gvfs mount of the phone with this serial (which `scan` does not show: it opens directly).
pub fn gvfs_mount(serial: &str) -> Option<String> {
    let run = std::env::var_os("XDG_RUNTIME_DIR")?;
    entries(&Path::new(&run).join("gvfs"))
        .into_iter()
        .find(|(entry, _)| gvfs_name(entry).and_then(|(_, s)| s).as_deref() == Some(serial))
        .map(|(_, path)| path)
}

pub fn scan() -> Vec<Device> {
    let mut devices = Vec::new();
    if cfg!(target_os = "linux") {
        devices.extend(external_disks());
    }
    #[cfg(windows)]
    devices.extend(drives());
    // Phones on USB: listing them reads only the descriptors, it does not open the device.
    let usb = mtp_rs::mtp::MtpDevice::list_devices().unwrap_or_default();
    let mut mounted = Vec::new();
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR") {
        for (entry, path) in entries(&Path::new(&run).join("gvfs")) {
            if let Some((name, serial)) = gvfs_name(&entry) {
                // A phone the direct connection sees opens directly even if gvfs mounted it
                // (`mtp::release_from_desktop`): the mount remains only for the others (iPhone).
                let direct = serial.as_ref().is_some_and(|s| usb.iter().any(|d| d.serial_number.as_ref() == Some(s)));
                if direct && !entry.starts_with("afc:") {
                    continue;
                }
                mounted.extend(serial.clone());
                devices.push(Device { name, path, phone: true, serial, block: None, internal: false });
            }
        }
    }
    // Direct connection. Without a serial there would be no way to reopen it.
    for d in usb {
        let Some(serial) = d.serial_number.filter(|s| !mounted.contains(s)) else { continue };
        let name = [d.manufacturer, d.product].into_iter().flatten().collect::<Vec<_>>().join(" ");
        let name = if name.is_empty() { serial.clone() } else { name };
        let path = format!("mtp://{serial}/");
        devices.push(Device { name, path, phone: true, serial: Some(serial), block: None, internal: false });
    }
    devices.sort_by_key(|d| d.name.to_lowercase());
    devices
}

/// Checks every 2 s and notifies the UI only when the list changes. Separate thread: a hung gvfs
/// mount (unreachable phone) can block the read without stopping the rest.
pub fn watch(tx: Sender<Msg>, ctx: Context) {
    let _ = std::thread::Builder::new().name("devices".into()).spawn(move || {
        let mut last = None;
        loop {
            let now = scan();
            if last.as_ref() != Some(&now) {
                if tx.send(Msg::Devices(now.clone())).is_err() {
                    return;
                }
                ctx.request_repaint();
                last = Some(now);
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let name = |e| gvfs_name(e).map(|(n, s)| (n, s.unwrap_or_default()));
        assert_eq!(name("gphoto2:host=Xiaomi_POCO_X7_Pro_ABCD1234EFGH5678"), Some(("Xiaomi POCO X7 Pro".into(), "ABCD1234EFGH5678".into())));
        assert_eq!(name("mtp:host=SAMSUNG_Galaxy_S24_R5CX12345AB"), Some(("SAMSUNG Galaxy S24".into(), "R5CX12345AB".into())));
        assert_eq!(name("mtp:host=Pixel"), Some(("Pixel".into(), String::new())));
        assert_eq!(gvfs_name("smb-share:server=nas,share=photos"), None);
        assert_eq!(unescape_mount("/mnt/Sea\\040photos"), "/mnt/Sea photos");
        let mounts = "/dev/nvme0n1p3 / btrfs rw 0 0\n/dev/sda2 /run/media/u/My\\040Disk fuseblk rw 0 0\n/dev/sda1 /mnt/Linux ext4 rw 0 0\n";
        assert!(ntfs_in(mounts, "/run/media/u/My Disk/photos/a.jpg"));
        assert!(!ntfs_in(mounts, "/run/media/u/My Diskette/a.jpg"));
        assert!(!ntfs_in(mounts, "/mnt/Linux/a.jpg"));
        assert!(!ntfs_in(mounts, "/home/u/a.jpg"));
        assert_eq!(unescape_mount("/mnt/Linux"), "/mnt/Linux");
        assert_eq!(unescape_udev("MY\\x20DISK\\x2020\\x2f07"), "MY DISK 20/07");
    }

    #[test]
    fn unplugged() {
        let disk = |path: &str| Device { name: "Stick".into(), path: path.into(), phone: false, serial: None, block: None, internal: false };
        let old = [disk("/run/media/u/STICK/"), disk("/mnt/Linux/")];
        let new = [disk("/mnt/Linux/")];
        assert!(gone_under(&old, &new, "/run/media/u/STICK/DCIM/"));
        assert!(gone_under(&old, &new, "/run/media/u/STICK/"));
        assert!(!gone_under(&old, &new, "/run/media/u/STICKER/"));
        assert!(!gone_under(&old, &new, "/mnt/Linux/photos/"));
        assert!(!gone_under(&old, &old, "/run/media/u/STICK/DCIM/"));
        // Windows: a drive is its own root.
        assert!(gone_under(&[disk("D:\\")], &[], "D:\\Photos\\"));
        // Unmounted partitions have no path: never a match.
        assert!(!gone_under(&[disk("")], &[], "/home/u/"));
    }

    /// What it sees now: `cargo test devices_now -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn devices_now() {
        for d in mtp_rs::mtp::MtpDevice::list_devices().unwrap() {
            println!("USB MTP: {:?} {:?} serial {:?}", d.manufacturer, d.product, d.serial_number);
        }
        println!("{:#?}", scan());
    }
}
