#!/bin/bash
# The video engine for the AppImage, downloaded by EyeCanSee when the system has no libmpv
# (src/component.rs): a small libmpv built in the AppImage's container (Ubuntu 22.04, glibc 2.35),
# with FFmpeg and dav1d inside it. Only what EyeCanSee uses: decoding and demuxing, the
# software renderer, PulseAudio and ALSA sound; no windows of its own, no scripts, no network.
# Output: target/mpv-linux/libmpv-linux-x64-<mpv>.tar.gz (libmpv.so.2 and the few libraries it
# needs that a desktop may lack, with RUNPATH $ORIGIN).
set -euo pipefail
MPV=0.35.1 FFMPEG=6.0.1 DAV1D=1.5.4

if [[ ! -f /.dockerenv && ! -f /run/.containerenv ]]; then
    mkdir -p target/mpv-linux
    exec podman run --rm -v "$PWD":/src:z -v "${APPIMAGE_CACHE:-eyecansee-appimage-cache}":/cache -w /src \
        docker.io/library/ubuntu:22.04 bash tools/build-mpv-linux.sh
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential nasm curl pkg-config meson ninja-build python3 patchelf \
    libass-dev libpulse-dev libasound2-dev zlib1g-dev >/dev/null

B=$(mktemp -d -p /cache)  # not /tmp: meson copies its per-container SELinux label
P=$B/prefix
export PKG_CONFIG_PATH=$P/lib/pkgconfig

curl -sSfL https://code.videolan.org/videolan/dav1d/-/archive/$DAV1D/dav1d-$DAV1D.tar.gz | tar xz -C "$B"
meson setup "$B/dav1d" "$B/dav1d-$DAV1D" --buildtype=release --prefix=$P --libdir=lib \
    --default-library=static -Db_staticpic=true -Denable_tools=false -Denable_tests=false >/dev/null
ninja -C "$B/dav1d" install >/dev/null

curl -sSfL https://ffmpeg.org/releases/ffmpeg-$FFMPEG.tar.xz | tar xJ -C "$B"
(cd "$B/ffmpeg-$FFMPEG" && ./configure --prefix=$P --enable-pic --enable-static --disable-shared \
    --disable-programs --disable-doc --disable-network --disable-avdevice --disable-encoders \
    --disable-muxers --disable-devices --disable-hwaccels --disable-vaapi --disable-vdpau \
    --disable-xlib --disable-libxcb --disable-sdl2 --disable-iconv --disable-autodetect --enable-zlib \
    --enable-libdav1d >/dev/null && make -j"$(nproc)" >/dev/null && make install >/dev/null)

curl -sSfL https://github.com/mpv-player/mpv/archive/refs/tags/v$MPV.tar.gz | tar xz -C "$B"
# FFmpeg is only static here: its private libraries (dav1d, zlib, libm) must be linked too.
printf '#!/bin/sh\nexec pkg-config --static "$@"\n' > "$B/pkg-config-static" && chmod +x "$B/pkg-config-static"
PKG_CONFIG="$B/pkg-config-static" LDFLAGS="-Wl,-Bsymbolic" meson setup "$B/mpv" "$B/mpv-$MPV" --buildtype=release --prefix=$P --libdir=lib \
    -Dlibmpv=true -Dcplayer=false -Dbuild-date=false -Dmanpage-build=disabled -Dhtml-build=disabled \
    -Dlua=disabled -Djavascript=disabled -Dlibarchive=disabled -Dlibbluray=disabled -Ddvdnav=disabled \
    -Dcdda=disabled -Duchardet=disabled -Drubberband=disabled -Dlcms2=disabled -Dzimg=disabled \
    -Djpeg=disabled -Dvapoursynth=disabled -Dsdl2=disabled -Dx11=disabled -Dwayland=disabled \
    -Dgl=disabled -Degl=disabled -Dvulkan=disabled -Dlibplacebo=disabled -Ddrm=disabled -Dgbm=disabled \
    -Dvaapi=disabled -Dvdpau=disabled -Dcuda-hwaccel=disabled -Dpipewire=disabled -Djack=disabled \
    -Dopenal=disabled -Dsndio=disabled -Doss-audio=disabled -Dpulse=enabled -Dalsa=enabled \
    -Dlibavdevice=disabled -Dzlib=enabled > "$B/mpv-setup.log" || { tail -30 "$B/mpv-setup.log"; exit 1; }
ninja -C "$B/mpv" install >/dev/null

OUT=$B/out
mkdir -p "$OUT"
cp -L $P/lib/libmpv.so.2 "$OUT/"
# Libraries a desktop may lack (libass and what it uses); the rest (glibc, freetype, fontconfig,
# PulseAudio, ALSA) every desktop has.
for lib in libass.so.9 libfribidi.so.0 libharfbuzz.so.0 libgraphite2.so.3; do
    cp -L "/usr/lib/x86_64-linux-gnu/$lib" "$OUT/"
done
for f in "$OUT"/*; do strip --strip-unneeded "$f"; patchelf --set-rpath '$ORIGIN' "$f"; done
echo "== needed"
for f in "$OUT"/*; do readelf -d "$f" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'; done | sort -u | tr '\n' ' '
echo
cp "$B/mpv-$MPV/LICENSE.GPL" "$OUT/LICENSE.mpv" 2>/dev/null || cp "$B/mpv-$MPV/Copyright" "$OUT/LICENSE.mpv"
cp "$B/ffmpeg-$FFMPEG/COPYING.LGPLv2.1" "$OUT/LICENSE.ffmpeg"
cp "$B/dav1d-$DAV1D/COPYING" "$OUT/LICENSE.dav1d"
NAME=libmpv-linux-x64-$MPV.tar.gz
tar -C "$OUT" --owner=0 --group=0 --sort=name --mtime=@0 -czf "/src/target/mpv-linux/$NAME" .
ls -la "$OUT" "/src/target/mpv-linux/$NAME"
sha256sum "/src/target/mpv-linux/$NAME"
rm -rf "$B"
