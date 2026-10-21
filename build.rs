//! On Windows embeds the icon (File Explorer, Desktop, Start) and the version data taken from
//! `Cargo.toml` into the executable. On Linux the .desktop file points to the icon (`install-linux.sh`).
//! `assets/eyecansee.ico` is regenerated from the SVG with the test `decode::tests::make_ico`.

fn main() {
    println!("cargo:rerun-if-changed=assets/eyecansee.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/eyecansee.ico");
        res.compile().expect("Windows resources (icon)");
    }
}
