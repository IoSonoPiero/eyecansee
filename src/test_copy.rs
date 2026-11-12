//! The private test copy (`--features test-copy`): built from the work in progress for a few
//! testers, never in a public package (`build.rs` stops it on GitHub Actions). It looks like the
//! published EyeCanSee except for its red icon, and it can run next to it on the same PC: its
//! folders (settings, servers, thumbnails, components, kept edits, temporary video copies), its
//! app id on Linux and the socket that finds a running copy carry another name. It never updates
//! itself (`update::Kind::detect`): a new test copy is installed by hand.

/// Name of the app's folders, of its app id on Linux and of its single-instance socket.
pub const NAME: &str = if cfg!(feature = "test-copy") { "eyecansee-prova" } else { "eyecansee" };

/// The app icon (window, task bar, About); the Windows executable's is in `build.rs`.
pub const ICON_SVG: &[u8] = if cfg!(feature = "test-copy") {
    include_bytes!("../assets/eyecansee-prova.svg")
} else {
    include_bytes!("../assets/eyecansee.svg")
};

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Without the feature everything keeps today's names; with it, the test copy's.
    #[test]
    fn names() {
        let name = if cfg!(feature = "test-copy") { "eyecansee-prova" } else { "eyecansee" };
        assert_eq!(super::NAME, name);
        if let Some(p) = crate::settings::config_path("settings.json") {
            assert!(p.ends_with(Path::new(name).join("settings.json")), "{p:?}");
        }
        if let Some(p) = crate::component::dir() {
            assert!(p.ends_with(Path::new(name).join("components")), "{p:?}");
        }
        assert_eq!(crate::thumbcache::dir_in(Path::new("base")), Path::new("base").join(name));
        assert!(crate::single::socket().starts_with(&format!("{name}-")));
        assert!(crate::video_read::temp_prefix().starts_with(&format!("{name}-video-")));
    }

    /// The icon is red only in the test copy.
    #[test]
    fn icon() {
        let svg = std::str::from_utf8(super::ICON_SVG).unwrap();
        assert_eq!(svg.contains("#7f1d1d"), cfg!(feature = "test-copy"));
        assert_eq!(svg.contains("#1d3557"), !cfg!(feature = "test-copy"));
    }

    /// The test copy never updates itself; the published one still finds how it was installed.
    #[test]
    #[cfg(feature = "test-copy")]
    fn no_updates() {
        assert!(crate::update::Kind::detect().is_none());
    }
}
