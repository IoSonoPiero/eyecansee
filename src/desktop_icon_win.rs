//! The desktop icon on Windows (#210), the system's part: the desktop folders, the shortcuts on
//! them read and written through the shell's COM objects (`IShellLinkW`, `IPersistFile`), and the
//! AppUserModelID of the Store package. The rules are in `desktop_icon.rs`.
//! [Unverified] Not run yet: to be tried in a Windows VM, with the `#[ignore]` test below, before
//! it reaches `main`; above all the Store shortcut (`SHParseDisplayName` + `SetIDList`) and how an
//! icon dragged from Start reads back.

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_PublicDesktop, ILFree, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, SHGetNameFromIDList, SHParseDisplayName, SIGDN_DESKTOPABSOLUTEPARSING, ShellLink};
use windows::core::{HSTRING, Interface, PCWSTR};

use crate::desktop_icon::{Link, Spec, Target, target_of, works};

fn err(e: windows::core::Error) -> String {
    e.message()
}

/// Runs `f` on a thread of its own, with its own COM apartment: never on the window's thread,
/// where a `CoUninitialize` closed the program once (#170).
// ponytail: the window waits for the thread (a few shortcuts, milliseconds); a message back to
// the window if a desktop with many shortcuts on a slow disk ever makes it felt.
pub(crate) fn com<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    std::thread::spawn(move || {
        // SAFETY: this thread's own apartment, closed below only when it was opened.
        let opened = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let r = f();
        if opened {
            // SAFETY: matches the CoInitializeEx above, on the same thread.
            unsafe { CoUninitialize() };
        }
        r
    })
    .join()
    .unwrap_or_else(|_| Err("the desktop icon's thread stopped".into()))
}

/// The AppUserModelID of the running package (the Store copy), read from the package: a test
/// package signed with another certificate gets its own.
pub(crate) fn aumid() -> Option<String> {
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentApplicationUserModelId;
    let mut len = 0u32;
    // SAFETY: a null buffer with length 0 only asks for the length.
    unsafe { GetCurrentApplicationUserModelId(&mut len, std::ptr::null_mut()) };
    let mut buf = vec![0u16; len as usize];
    // SAFETY: the buffer is `len` characters long, as asked.
    if len == 0 || unsafe { GetCurrentApplicationUserModelId(&mut len, buf.as_mut_ptr()) } != 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]))
}

/// The user's desktop or the public one, as the system says: OneDrive can move the first.
pub(crate) fn desktop(public: bool) -> Result<PathBuf, String> {
    let id = if public { &FOLDERID_PublicDesktop } else { &FOLDERID_Desktop };
    // SAFETY: the returned string is the system's, freed with CoTaskMemFree after the copy.
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(err)?;
        let path = p.to_string().map_err(|e| e.to_string());
        CoTaskMemFree(Some(p.0 as _));
        path.map(PathBuf::from)
    }
}

/// `%SystemRoot%`, where PowerShell is.
pub(crate) fn system_root() -> Result<PathBuf, String> {
    std::env::var_os("SystemRoot").map(PathBuf::from).ok_or_else(|| "SystemRoot is not set".into())
}

/// The shortcuts on the user's desktop and on the public one. One that cannot be read counts as
/// working (`Target::Unread`): its name is never taken over.
pub(crate) fn links() -> Result<Vec<Link>, String> {
    let mut all = Vec::new();
    for public in [false, true] {
        let Ok(folder) = desktop(public) else { continue };
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk"))) {
            let target = read(&path).unwrap_or_else(|e| {
                log!("desktop icon: {} not read: {e}", path.display());
                Target::Unread
            });
            let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            all.push(Link { name, works: works(&target, Path::exists), target, public });
        }
    }
    Ok(all)
}

fn link() -> Result<IShellLinkW, String> {
    // SAFETY: the shell's own class, in this thread's apartment (`com`).
    unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.map_err(err)
}

/// What a shortcut starts: a program (with PowerShell, the script it runs), or the app of a
/// package for a shortcut with no path, only the shell item of the app (one dragged from Start).
pub(crate) fn read(path: &Path) -> Result<Target, String> {
    let link = link()?;
    let file = HSTRING::from(path.as_os_str());
    // SAFETY: COM calls on objects of this thread, buffers sized as passed.
    unsafe {
        link.cast::<IPersistFile>().map_err(err)?.Load(PCWSTR(file.as_ptr()), STGM_READ).map_err(err)?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).map_err(err)?;
        let program = wide(&buf);
        if !program.is_empty() {
            let mut args = [0u16; 2048];
            link.GetArguments(&mut args).map_err(err)?;
            return Ok(target_of(Path::new(&program), &wide(&args)));
        }
        let pidl = link.GetIDList().map_err(err)?;
        let name = SHGetNameFromIDList(pidl, SIGDN_DESKTOPABSOLUTEPARSING);
        ILFree(Some(pidl));
        let name = name.map_err(err)?;
        let parsed = name.to_string().map_err(|e| e.to_string());
        CoTaskMemFree(Some(name.0 as _));
        // [Unverified] The parsing name of an app in AppsFolder: "<AppsFolder's id>\<AUMID>".
        let parsed = parsed?;
        let aumid = parsed.rsplit('\\').next().filter(|a| a.contains('!')).ok_or_else(|| format!("not an app: {parsed}"))?;
        Ok(Target::App(aumid.to_string()))
    }
}

/// Writes the shortcut `path` (".lnk"); one already there with that name is replaced.
pub(crate) fn write(path: &Path, spec: &Spec) -> Result<(), String> {
    let link = link()?;
    let w = |s: &std::ffi::OsStr| HSTRING::from(s);
    // SAFETY: COM calls on objects of this thread; the strings live until the calls return.
    unsafe {
        match &spec.target {
            Target::Exe(program) => link.SetPath(PCWSTR(w(program.as_os_str()).as_ptr())).map_err(err)?,
            Target::Script(_) | Target::Unread => return Err("a script is written as PowerShell with its arguments".into()),
            // [Unverified] The shell item of the app, as the icon dragged from Start has it; the
            // fallback, if the VM says no: `explorer.exe shell:AppsFolder\<AUMID>` with our icon.
            Target::App(aumid) => {
                let item = HSTRING::from(format!(r"shell:AppsFolder\{aumid}"));
                let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
                SHParseDisplayName(PCWSTR(item.as_ptr()), None, &mut pidl, 0, None).map_err(err)?;
                let set = link.SetIDList(pidl);
                ILFree(Some(pidl));
                set.map_err(err)?;
            }
        }
        if !spec.args.is_empty() {
            link.SetArguments(PCWSTR(HSTRING::from(spec.args.as_str()).as_ptr())).map_err(err)?;
        }
        if !matches!(spec.target, Target::App(_)) {
            link.SetWorkingDirectory(PCWSTR(w(spec.dir.as_os_str()).as_ptr())).map_err(err)?;
            link.SetIconLocation(PCWSTR(w(spec.icon.as_os_str()).as_ptr()), 0).map_err(err)?;
        }
        link.cast::<IPersistFile>().map_err(err)?.Save(PCWSTR(w(path.as_os_str()).as_ptr()), true).map_err(err)
    }
}

/// A string of the system up to its end.
fn wide(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_icon::Target;

    /// In a Windows VM: a shortcut written in a temporary folder, not on the desktop, reads back
    /// with what it starts; then deleted. `cargo test desktop_icon_live -- --ignored --nocapture`.
    #[test]
    #[ignore = "Windows shell, in a VM"]
    fn desktop_icon_live() {
        let dir = std::env::temp_dir().join("eyecansee-desktop-icon-test");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = std::env::current_exe().unwrap();
        let spec = Spec { target: Target::Exe(exe.clone()), args: String::new(), dir: exe.parent().unwrap().into(), icon: exe.clone() };
        let path = dir.join("EyeCanSee.lnk");
        let back = com(move || {
            write(&path, &spec)?;
            read(&path)
        });
        println!("read back: {back:?}");
        let ps = system_root().unwrap().join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let script = Spec { target: Target::Exe(ps), args: r#"-NoProfile -File "C:\P\avvia.ps1""#.into(), dir: dir.clone(), icon: exe.clone() };
        let path2 = dir.join("EyeCanSee prova.lnk");
        let back2 = com(move || {
            write(&path2, &script)?;
            read(&path2)
        });
        println!("script read back: {back2:?}");
        println!("desktop: {:?}, public: {:?}", desktop(false), desktop(true));
        println!("links: {:#?}", com(links));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(back.unwrap(), Target::Exe(exe));
        assert_eq!(back2.unwrap(), Target::Script(r"C:\P\avvia.ps1".into()));
    }
}
