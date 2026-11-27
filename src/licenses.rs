//! The licences of the libraries EyeCanSee is built with: the THIRD-PARTY-LICENSES.txt every
//! package installs near the program, opened from Help > About. A copy built from source has
//! none, and About shows no link then. The tests check the Windows installer's notice page too.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The file's name, the same in every package.
pub(crate) const FILE: &str = "THIRD-PARTY-LICENSES.txt";

/// Where each package puts it, from the folder of the executable: next to it (zip, installer and
/// MSIX, `tools/eyecansee.iss`; the test copy, `tools/test-copy/install-linux.sh`), the AppImage
/// (`usr/bin` → `usr/share/doc/eyecansee`, `tools/build-appimage.sh`), the Fedora package
/// (`%license`, `/usr/share/licenses/eyecansee`), the Flatpak (`/app/share/licenses/<app id>`).
pub(crate) const PLACES: [&str; 4] = ["", "../share/doc/eyecansee", "../share/licenses/eyecansee", "../share/licenses/io.github.IoSonoPiero.EyeCanSee"];

/// The file, for the program in `exe_dir`: the first of `PLACES` that holds it.
pub(crate) fn find_from(exe_dir: &Path) -> Option<PathBuf> {
    PLACES.iter().map(|p| exe_dir.join(p).join(FILE)).find(|f| f.is_file())
}

/// The file of this copy, looked for once (About is drawn at every frame).
pub(crate) fn find() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let exe = std::env::current_exe().ok()?;
            let found = find_from(exe.parent()?);
            log!("licenses: {found:?}");
            found
        })
        .as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each package's place, found from its executable's folder; none for a copy built from
    /// source.
    #[test]
    fn found_where_each_package_puts_it() {
        let root = std::env::temp_dir().join(format!("eyecansee-licenses-{}", std::process::id()));
        let cases = [
            ("zip", "EyeCanSee", "EyeCanSee"),
            ("appimage", "usr/bin", "usr/share/doc/eyecansee"),
            ("rpm", "usr/bin", "usr/share/licenses/eyecansee"),
            ("flatpak", "app/bin", "app/share/licenses/io.github.IoSonoPiero.EyeCanSee"),
        ];
        for (name, bin, at) in cases {
            let base = root.join(name);
            std::fs::create_dir_all(base.join(bin)).unwrap();
            std::fs::create_dir_all(base.join(at)).unwrap();
            assert_eq!(find_from(&base.join(bin)), None, "{name}: found before it was there");
            std::fs::write(base.join(at).join(FILE), "licences").unwrap();
            let found = find_from(&base.join(bin)).unwrap_or_else(|| panic!("{name}: not found"));
            assert_eq!(std::fs::read_to_string(found).unwrap(), "licences", "{name}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The places above are those the packages use: if a script moves the file, this fails.
    #[test]
    fn places_match_the_packages() {
        let iss = include_str!("../tools/eyecansee.iss");
        assert!(iss.contains(r#"Source: "{#Pkg}\THIRD-PARTY-LICENSES.txt"; DestDir: "{app}""#), "installer");
        assert!(iss.contains(r#"Source: "{#Pkg}\eyecansee.exe"; DestDir: "{app}""#), "installer");
        let appimage = include_str!("../tools/build-appimage.sh");
        assert!(appimage.contains("LICENSES=$APPDIR/usr/share/doc/eyecansee/THIRD-PARTY-LICENSES.txt"), "AppImage");
        assert!(appimage.contains("-e $PROFILE/eyecansee"), "AppImage: linuxdeploy puts the executable in usr/bin");
        let spec = include_str!("../eyecansee.spec");
        assert!(spec.contains("\nName:           eyecansee\n") && spec.contains("%{_bindir}/eyecansee"), "RPM");
        assert!(spec.lines().any(|l| l.starts_with("%license") && l.contains(FILE)), "RPM");
        let flatpak = include_str!("../flatpak/io.github.IoSonoPiero.EyeCanSee.yml");
        assert!(flatpak.contains("/app/bin/eyecansee") && flatpak.contains(&format!("/app/share/licenses/io.github.IoSonoPiero.EyeCanSee/{FILE}")), "Flatpak");
        let test_copy = include_str!("../tools/test-copy/install-linux.sh");
        assert!(test_copy.contains(&format!("\"$DIR/{FILE}\"")) && test_copy.contains("\"$DIR/eyecansee\""), "test copy");
    }

    /// The Windows installer's notice page, before choosing the folder: one text per language of
    /// the installer, UTF-8 with a BOM as Inno Setup reads it, with the licence, no warranty and
    /// the backup. Updates run the installer with /SILENT, which shows no wizard page.
    #[test]
    fn installer_notice_in_every_language() {
        let iss = include_str!("../tools/eyecansee.iss");
        let section = iss.split("[Languages]").nth(1).and_then(|s| s.split("\n[").next()).expect("no [Languages]");
        let languages: Vec<&str> = section.lines().filter(|l| l.starts_with("Name:")).collect();
        assert_eq!(languages.len(), 2, "{languages:?}");
        let says = [("en", ["NO WARRANTY", "keep a backup", "THIRD-PARTY-LICENSES.txt"]), ("it", ["SENZA ALCUNA GARANZIA", "tieni una copia", "THIRD-PARTY-LICENSES.txt"])];
        for (lang, words) in says {
            let line = languages.iter().find(|l| l.starts_with(&format!("Name: \"{lang}\";"))).unwrap_or_else(|| panic!("{lang}: not in the installer"));
            let file = format!("installer-info.{lang}.txt");
            assert!(line.contains(&format!("InfoBeforeFile: \"{file}\"")), "{lang}: {line}");
            let bytes = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tools").join(&file)).unwrap();
            let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or_else(|| panic!("{file}: no BOM"))).unwrap();
            for w in words.iter().chain(&["GNU General Public License", "LICENSE.txt"]) {
                assert!(text.contains(w), "{file}: no {w:?}");
            }
        }
        // Nothing in the installer skips the page by hand: /SILENT does it for the updates.
        assert!(!iss.contains("ShouldSkipPage"));
        assert!(include_str!("update.rs").contains("-ArgumentList '/SILENT'"), "updates no longer silent");
    }
}
