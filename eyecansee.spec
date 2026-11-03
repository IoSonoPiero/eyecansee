# RPM for Fedora, built on COPR (iosonopiero/eyecansee) by tools/copr-build.sh, which sets Version
# from the VERSION file. The COPR project has network access during builds: cargo downloads the
# crates listed in Cargo.lock (--locked), instead of shipping ~700 MB of vendored sources.
Name:           eyecansee
Version:        0
Release:        1%{?dist}
Summary:        Viewer and manager for photos, videos, 3D models and CAD drawings
License:        GPL-3.0-or-later
URL:            https://github.com/IoSonoPiero/eyecansee
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo >= 1.95
BuildRequires:  rust >= 1.95
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  cmake
BuildRequires:  nasm
BuildRequires:  clang-devel
BuildRequires:  pkgconfig(libheif) >= 1.23
BuildRequires:  pkgconfig(MagickWand) >= 7
BuildRequires:  desktop-file-utils
BuildRequires:  appstream
# HEIC needs the HEVC decoder plugin, which Fedora does not ship (patents): RPM Fusion has it.
Recommends:     libheif-freeworld
# Videos: libmpv, opened at run time (src/mpv.rs). HEVC videos need RPM Fusion's ffmpeg too.
Recommends:     mpv-libs

# The release profile strips the executable: there is no debug information to package.
%global debug_package %{nil}
# Fedora's LTO flags reach the C libraries built by cargo (libjpeg-turbo): on Fedora 43 GCC 15.3
# crashed linking them ("lto1: internal compiler error"). Rust code is not affected.
%global _lto_cflags %{nil}

%description
EyeCanSee is a viewer and manager for photos, videos, 3D models and CAD drawings, written in
Rust. It browses them in local folders, on external disks, on SFTP and FTP servers and on
Android phones connected directly over USB (MTP), with thumbnails and a full-screen viewer;
it copies and moves them from one device to another, finds duplicate copies and converts
them to other formats.

%prep
%autosetup

%build
export CARGO_HOME=%{_builddir}/cargo-home
cargo build --release --locked

%install
install -Dm755 target/release/eyecansee %{buildroot}%{_bindir}/eyecansee
install -Dm644 assets/eyecansee.desktop %{buildroot}%{_datadir}/applications/eyecansee.desktop
install -Dm644 assets/eyecansee.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/eyecansee.svg
install -Dm644 assets/io.github.IoSonoPiero.EyeCanSee.metainfo.xml %{buildroot}%{_metainfodir}/io.github.IoSonoPiero.EyeCanSee.metainfo.xml

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/eyecansee.desktop
appstreamcli validate --no-net %{buildroot}%{_metainfodir}/io.github.IoSonoPiero.EyeCanSee.metainfo.xml

%files
%license LICENSE
%doc README.md
%{_bindir}/eyecansee
%{_datadir}/applications/eyecansee.desktop
%{_datadir}/icons/hicolor/scalable/apps/eyecansee.svg
%{_metainfodir}/io.github.IoSonoPiero.EyeCanSee.metainfo.xml
