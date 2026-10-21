# EyeCanSee

An image viewer for Linux and Windows, written in Rust. Free and open source.

It is early in development: today it **views** images, it does not edit them yet. This page lists only what works now. The plan is to finish the viewing part first, then add the first editing features step by step, each one only when it is ready and tested.

## About

I am not a programmer any more, and have not been for years. I needed an image viewer that could open the photos on my phone and on my disks quickly, on both Linux and Windows, so I started writing EyeCanSee with the help of [Claude Code](https://claude.com/claude-code), an AI assistant. I test every feature on my own computers and devices before it goes in, and the manual says what is known not to work.

Bug reports and suggestions are welcome in the [issues](https://github.com/IoSonoPiero/eyecansee/issues).

## Why another image viewer?

Mature viewers like XnView, IrfanView or digiKam do much more than EyeCanSee, and if you need editing, catalogues or batch tools they are the better choice today. EyeCanSee started from one need: browsing the photos on an Android phone quickly. It talks to the phone directly over USB instead of going through the file manager: on Linux, a folder with hundreds of photos that took minutes now takes seconds. It works the same way on Linux and Windows, and it is open source.

## What it does today

- **Browse folders** with a folder tree on the left and a grid of thumbnails on the right, in the style of XnView.
- **View images** full window, moving to the next and previous image with the keyboard. The images around the current one are loaded in advance.
- **Thumbnails** are saved on disk, so a folder opens immediately the second time.
- **Phones and cameras** connected via USB (Android, in "File transfer" or "Transfer photos" mode) are opened directly, without the file manager. Tested with a Xiaomi POCO X7 Pro on Fedora and on Windows 11.
- **External disks, USB sticks and SD cards** appear by themselves when connected.
- **EXIF metadata** of the selected photo: camera, exposure, date, GPS position with a link to OpenStreetMap.
- **Sorting** by name, date, size or type.
- **Interface** in English and Italian, light and dark theme, built-in manual (F1).

### Formats

- On Linux: JPEG, PNG, WebP, GIF, BMP, TIFF, HEIC, AVIF, JPEG XL, SVG, and many less common formats through ImageMagick (PSD, XCF, JPEG 2000, PCX and others).
- On Windows: the same, **except** HEIC, AVIF and the ImageMagick formats, which are not included yet.
- Camera RAW files (CR2, CR3, NEF, ARW, DNG and others) are supported by the code, but have not been tested with real camera files yet.

The full list of features, keys and limits is in the manual: [English](docs/manual.en.md), [Italiano](docs/manual.it.md).

## Download

- **Windows**: download the zip from the most recent [release](https://github.com/IoSonoPiero/eyecansee/releases) that has one (the Windows build sometimes follows a few days later), extract it anywhere and run `eyecansee.exe`. No installation needed. The program is not signed, so Windows may warn you the first time: click **More info**, then **Run anyway**.
- **Fedora** (43 and later): install it from the [COPR repository](https://copr.fedorainfracloud.org/coprs/iosonopiero/eyecansee/), and updates arrive with the system ones:
  ```bash
  sudo dnf copr enable iosonopiero/eyecansee
  sudo dnf install eyecansee
  ```
  HEIC photos also need `libheif-freeworld` from [RPM Fusion](https://rpmfusion.org/), which `dnf` installs by itself if RPM Fusion is enabled.
- **Other Linux distributions**: download the `.AppImage` file from the [latest release](https://github.com/IoSonoPiero/eyecansee/releases/latest), make it executable (`chmod +x EyeCanSee-*.AppImage`, or Properties → "Allow executing file as program" in the file manager) and run it. Nothing else to install. It needs a distribution from 2022 or later (glibc 2.35).

## Build

### Linux (Fedora)

```bash
sudo dnf install libheif-devel ImageMagick-devel clang-devel cmake nasm
cargo build --release
./target/release/eyecansee ~/Pictures
```

`./install-linux.sh` builds and installs it for the current user (applications menu, "Open with"), without `sudo`. `./install-linux.sh --uninstall` removes it.

`tools/copr-build.sh` builds the source RPM from `eyecansee.spec` and sends it to COPR. `tools/build-appimage.sh` builds the AppImage in an Ubuntu 22.04 container (needs `podman`), with libheif, its decoders and ImageMagick compiled from source.

### Windows

In PowerShell, once:

```powershell
winget install --id Git.Git -e
winget install --id Rustlang.Rustup -e
winget install --id Kitware.CMake -e
winget install --id NASM.NASM -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Then open a new PowerShell window (so that `PATH` is reloaded) and check that every tool answers:

```powershell
git --version; cargo --version; cmake --version; nasm -v
```

If `nasm` is not found, add its folder to `PATH` for the current user and open a new window. The folder depends on the NASM installer: look for `nasm.exe` in `%LOCALAPPDATA%\bin\NASM` or `C:\Program Files\NASM`. For example:

```powershell
[Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path", "User") + ";$env:LOCALAPPDATA\bin\NASM", "User")
```

Build and run (`--no-default-features` leaves out HEIC/AVIF and ImageMagick, see *Formats*):

```powershell
git clone https://github.com/IoSonoPiero/eyecansee.git
cd eyecansee
cargo build --release --no-default-features
.\target\release\eyecansee.exe $env:USERPROFILE\Pictures
```

## Translations

The interface is available in English and Italian. To add a language, translate `locales/en.ftl` and `docs/manual.en.md`, then add one line to `LANGS` in `src/i18n.rs`. `cargo test` checks that every language has the same messages and manual sections as English.

## License

EyeCanSee is released under the GNU General Public License, version 3 or later: see [LICENSE](LICENSE).
