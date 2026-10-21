#!/usr/bin/env bash
# Builds EyeCanSee-<version>-x86_64.AppImage in an Ubuntu 22.04 container (podman), so that it runs on
# distributions with glibc 2.35 or newer. libheif is compiled into the executable (embedded-libheif);
# its decoders (libde265 for HEIC, dav1d for AVIF) are built from source at the versions below (Ubuntu's
# are years old) and shipped inside the AppImage. ImageMagick 7 (rare formats; Ubuntu 22.04 only has 6) is
# built from source too, with its coders inside the library; the format libraries it uses come from Ubuntu.
# LICENSE and THIRD-PARTY-LICENSES.txt go in usr/share/doc/eyecansee/ inside the AppImage.
# Usage: tools/build-appimage.sh   (output in target/appimage/)
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" != "--inside" ]]; then
    podman run --rm -v "$PWD":/src:z -v eyecansee-appimage-cache:/cache -w /src \
        docker.io/library/ubuntu:22.04 bash tools/build-appimage.sh --inside
    exit
fi

export DEBIAN_FRONTEND=noninteractive CARGO_HOME=/cache/cargo RUSTUP_HOME=/cache/rustup
export CARGO_TARGET_DIR=/src/target/appimage PATH=/cache/cargo/bin:$PATH
apt-get update -qq
apt-get install -y -qq build-essential cmake nasm curl file jq pkg-config meson ninja-build \
    libglib2.0-0 libclang-dev libpng-dev libjpeg-turbo8-dev libtiff-dev libwebp-dev libopenjp2-7-dev \
    liblcms2-dev libfreetype-dev libbz2-dev liblzma-dev zlib1g-dev >/dev/null
DE265=1.1.3 DAV1D=1.5.4
DEPS=/cache/deps-de265-$DE265-dav1d-$DAV1D
if [[ ! -d $DEPS ]]; then
    B=$(mktemp -d -p /cache)  # not /tmp: meson copies its per-container SELinux label
    curl -sSfL https://github.com/strukturag/libde265/archive/refs/tags/v$DE265.tar.gz | tar xz -C "$B"
    cmake -S "$B/libde265-$DE265" -B "$B/de265" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$DEPS \
        -DCMAKE_INSTALL_LIBDIR=lib -DENABLE_DECODER=OFF -DENABLE_ENCODER=OFF -DENABLE_SDL=OFF
    cmake --build "$B/de265" -j"$(nproc)" --target install
    curl -sSfL https://code.videolan.org/videolan/dav1d/-/archive/$DAV1D/dav1d-$DAV1D.tar.gz | tar xz -C "$B"
    meson setup "$B/dav1d" "$B/dav1d-$DAV1D" --buildtype=release --prefix=$DEPS --libdir=lib \
        -Denable_tools=false -Denable_tests=false
    ninja -C "$B/dav1d" install
    cp "$B/libde265-$DE265/COPYING" $DEPS/COPYING.libde265
    cp "$B/dav1d-$DAV1D/COPYING" $DEPS/COPYING.dav1d
    rm -rf "$B"
fi
IM=7.1.2-34
IMDIR=/cache/imagemagick-$IM
if [[ ! -d $IMDIR ]]; then
    B=$(mktemp -d -p /cache)
    curl -sSfL https://github.com/ImageMagick/ImageMagick/archive/refs/tags/$IM.tar.gz | tar xz -C "$B"
    (cd "$B/ImageMagick-$IM" && ./configure -q --prefix=$IMDIR --enable-shared --disable-static \
        --without-modules --enable-hdri --without-magick-plus-plus --without-perl --without-x --without-xml --disable-docs \
        && make -s -j"$(nproc)" install)
    cp "$B/ImageMagick-$IM/LICENSE" $IMDIR/COPYING.ImageMagick
    rm -rf "$B"
fi
export PKG_CONFIG_PATH=$DEPS/lib/pkgconfig:$IMDIR/lib/pkgconfig LD_LIBRARY_PATH=$DEPS/lib:$IMDIR/lib
export CMAKE_PREFIX_PATH=$DEPS
[[ -x /cache/cargo/bin/cargo ]] || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal

FEATURES=(--no-default-features --features heif,magick,libheif-rs/embedded-libheif)
# libheif caches the decoders it found in its CMake build folder: start clean every time.
rm -rf "$CARGO_TARGET_DIR"/release/build/libheif-sys-*
cargo build --release --locked "${FEATURES[@]}"

VER=$(cat VERSION)
APPDIR=$CARGO_TARGET_DIR/AppDir
rm -rf "$APPDIR"
install -Dm644 LICENSE "$APPDIR/usr/share/doc/eyecansee/LICENSE"
LICENSES=$APPDIR/usr/share/doc/eyecansee/THIRD-PARTY-LICENSES.txt
tools/third-party-licenses.sh "$LICENSES" "${FEATURES[@]}"
for lib in "libde265 $DE265 https://github.com/strukturag/libde265/archive/refs/tags/v$DE265.tar.gz" \
           "dav1d $DAV1D https://code.videolan.org/videolan/dav1d/-/archive/$DAV1D/dav1d-$DAV1D.tar.gz" \
           "ImageMagick $IM https://github.com/ImageMagick/ImageMagick/archive/refs/tags/$IM.tar.gz"; do
    set -- $lib
    printf '\n%s\n%s %s (shared library in usr/lib)\nSource: %s\n%s\n\n' "$(printf '=%.0s' {1..78})" "$1" "$2" "$3" \
        "$(printf '=%.0s' {1..78})" >> "$LICENSES"
    cat "$DEPS/COPYING.$1" "$IMDIR/COPYING.$1" 2>/dev/null >> "$LICENSES" || true
done
LD=/cache/linuxdeploy-x86_64.AppImage
[[ -x $LD ]] || { curl -sSfL -o $LD \
    https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
    chmod +x $LD; }
cd "$CARGO_TARGET_DIR"
APPIMAGE_EXTRACT_AND_RUN=1 LINUXDEPLOY_OUTPUT_VERSION=$VER $LD --appdir "$APPDIR" \
    -e release/eyecansee -d /src/assets/eyecansee.desktop -i /src/assets/eyecansee.svg --output appimage
chown "$(stat -c %u:%g /src)" EyeCanSee-*.AppImage
ls -l EyeCanSee-*.AppImage
