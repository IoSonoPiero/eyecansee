//! On Windows embeds the icon (File Explorer, Desktop, Start) and the version data taken from
//! `Cargo.toml` into the executable. On Linux the .desktop file points to the icon (`install-linux.sh`).
//! `assets/eyecansee.ico` is regenerated from the SVG with the test `decode::tests::make_ico`.
//! The private test copy (feature `test-copy`, `src/test_copy.rs`) has a red icon and is never
//! built on GitHub Actions, so it cannot end up in a release.

fn main() {
    println!("cargo:rerun-if-changed=assets/eyecansee.ico");
    println!("cargo:rerun-if-changed=assets/eyecansee-prova.ico");
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    let test_copy = std::env::var_os("CARGO_FEATURE_TEST_COPY").is_some();
    if test_copy && std::env::var_os("GITHUB_ACTIONS").is_some() {
        panic!("the test copy (feature test-copy) is never built on GitHub Actions");
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(if test_copy { "assets/eyecansee-prova.ico" } else { "assets/eyecansee.ico" });
        // Shown by "Open with" and in the file's properties: the package name is lowercase.
        res.set("FileDescription", "EyeCanSee");
        res.set("ProductName", "EyeCanSee");
        res.compile().expect("Windows resources (icon)");
    }
}
