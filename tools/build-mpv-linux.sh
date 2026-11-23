#!/bin/bash
# The video engine for the AppImage, downloaded by EyeCanSee when the system has no libmpv
# (src/component.rs): a small libmpv built in the AppImage's container (Ubuntu 22.04, glibc 2.35),
# with FFmpeg, dav1d and libplacebo inside it. Only what EyeCanSee uses: decoding and demuxing,
# the software renderer, PulseAudio and ALSA sound; no windows of its own, no scripts, no network.
# libplacebo is required by mpv since 0.37: here without Vulkan, OpenGL, shader compilers or
# LittleCMS, which the software renderer does not use.
# Output: target/mpv-linux/libmpv-linux-x64-<mpv>-<rev>.tar.gz (libmpv.so.2 and the few libraries it
# needs that a desktop may lack, with RUNPATH $ORIGIN).
set -euo pipefail
MPV=0.42.0 FFMPEG=9.0.2 DAV1D=1.5.5 PLACEBO=7.387.0 MESON=1.12.1
# The build of this mpv version: one more when a library inside changes (2: dav1d 1.5.5, #172).
REV=2
# What each source must be (tools/fetch.sh). To update a version: its new SHA-256 or commit here,
# checked against what the project publishes. FFmpeg: the release tarball, whose signature (FFmpeg's
# release key FCF986EA15E6E293A5644F10B4322F04D67658D8) was checked; FFmpeg publishes no SHA-256.
# dav1d: VideoLAN's release tarball and its published .sha256. mpv: the tag's archive, for which mpv
# publishes no hash (the commit inside it is the tag's). libplacebo: the commit its tag points to.
# meson: the SHA-256 that PyPI lists for its wheel and its source.
FFMPEG_SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
DAV1D_SHA256=ca96b1d0ba3f01f55de7231d775892b64994ba061aaeeaaa4f662a390b3abf9e
MPV_SHA256=bcfebbfa2b65a6d88dc97ea18117831072373e38b5785a2afb874ed4df5070dd
PLACEBO_COMMIT=275f7dea27924cdd34ab2e6c20e448b6bc4fab65
MESON_SHA256=(930bc7542cbd9f57009e182fd014eba48cf1a0180a7b9006d2d32d8f168d8b02
              ab0a6ca09f8ef70c564c8241fb5a23957886a0b53fb58412b5e07eaf07dba743)

if [[ ! -f /.dockerenv && ! -f /run/.containerenv ]]; then
    mkdir -p target/mpv-linux
    exec podman run --rm -v "$PWD":/src:z -v "${APPIMAGE_CACHE:-eyecansee-appimage-cache}":/cache -w /src \
        docker.io/library/ubuntu:22.04 bash tools/build-mpv-linux.sh
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential nasm curl git pkg-config ninja-build python3 python3-pip patchelf \
    libass-dev libpulse-dev libasound2-dev zlib1g-dev >/dev/null
source "$(dirname "$0")/fetch.sh"

B=$(mktemp -d -p /cache)  # not /tmp: meson copies its per-container SELinux label
trap 'rm -rf "$B"' EXIT
# mpv 0.42 and libplacebo want meson >= 1.3; Ubuntu 22.04 has 0.61.
echo "meson==$MESON ${MESON_SHA256[*]/#/--hash=sha256:}" > "$B/meson.txt"
pip3 install -q --no-cache-dir --require-hashes -r "$B/meson.txt"

P=$B/prefix
# A step's output goes to a log, shown only when it fails (ninja and make print errors on stdout).
quiet() { "$@" > "$B/step.log" 2>&1 || { tail -40 "$B/step.log"; exit 1; }; }
export PKG_CONFIG_PATH=$P/lib/pkgconfig

fetch https://downloads.videolan.org/pub/videolan/dav1d/$DAV1D/dav1d-$DAV1D.tar.xz $DAV1D_SHA256 "$B/dav1d.tar.xz"
tar xJf "$B/dav1d.tar.xz" -C "$B"
meson setup "$B/dav1d" "$B/dav1d-$DAV1D" --buildtype=release --prefix=$P --libdir=lib \
    --default-library=static -Db_staticpic=true -Denable_tools=false -Denable_tests=false >/dev/null
quiet ninja -C "$B/dav1d" install

# libplacebo from git at its tag: the tarball lacks the submodules. Only those the build reads:
# jinja and markupsafe for its code generator, fast_float compiled in, Vulkan-Headers for the
# types of its Vulkan stubs (headers only, nothing of Vulkan is linked); not glad (OpenGL).
git -c advice.detachedHead=false clone -q --depth 1 --branch v$PLACEBO https://code.videolan.org/videolan/libplacebo.git "$B/libplacebo-src"
# The tag must still point to the commit written above (the submodules' commits are fixed by it).
[[ $(git -C "$B/libplacebo-src" rev-parse HEAD) == "$PLACEBO_COMMIT" ]] \
    || { echo "libplacebo v$PLACEBO: not commit $PLACEBO_COMMIT" >&2; exit 1; }
git -C "$B/libplacebo-src" submodule -q update --init --depth 1 \
    3rdparty/jinja 3rdparty/markupsafe 3rdparty/fast_float 3rdparty/Vulkan-Headers
meson setup "$B/libplacebo" "$B/libplacebo-src" --buildtype=release --prefix=$P --libdir=lib \
    --default-library=static -Db_staticpic=true -Dvulkan=disabled -Dvk-proc-addr=disabled \
    -Dopengl=disabled -Dgl-proc-addr=disabled -Dd3d11=disabled -Dglslang=disabled -Dshaderc=disabled \
    -Dlcms=disabled -Dlibdovi=disabled -Dunwind=disabled -Dxxhash=disabled -Ddemos=false \
    -Dtests=false > "$B/libplacebo-setup.log" || { tail -30 "$B/libplacebo-setup.log"; exit 1; }
quiet ninja -C "$B/libplacebo" install
# libplacebo has some C++ (std::from_chars and to_chars for numbers in its options) that its
# .pc does not declare: libstdc++ linked in statically after it, so no libstdc++.so is needed.
PC=$P/lib/pkgconfig/libplacebo.pc
if grep -q '^Libs.private:' "$PC"; then sed -i '/^Libs.private:/ s/$/ -l:libstdc++.a/' "$PC"
else echo 'Libs.private: -l:libstdc++.a' >> "$PC"; fi

fetch https://ffmpeg.org/releases/ffmpeg-$FFMPEG.tar.xz $FFMPEG_SHA256 "$B/ffmpeg.tar.xz"
tar xJf "$B/ffmpeg.tar.xz" -C "$B"
(cd "$B/ffmpeg-$FFMPEG" && ./configure --prefix=$P --enable-pic --enable-static --disable-shared \
    --disable-programs --disable-doc --disable-network --disable-avdevice --disable-encoders \
    --disable-muxers --disable-devices --disable-hwaccels --disable-vaapi --disable-vdpau \
    --disable-xlib --disable-libxcb --disable-sdl2 --disable-iconv --disable-autodetect --enable-zlib \
    --enable-libdav1d >/dev/null)
quiet make -C "$B/ffmpeg-$FFMPEG" -j"$(nproc)" install

fetch https://github.com/mpv-player/mpv/archive/refs/tags/v$MPV.tar.gz $MPV_SHA256 "$B/mpv.tar.gz"
tar xzf "$B/mpv.tar.gz" -C "$B"
# FFmpeg is only static here: its private libraries (dav1d, zlib, libm) must be linked too.
# -static-libgcc: the unwinder that the static libstdc++ needs goes inside, no libgcc_s.so.
printf '#!/bin/sh\nexec pkg-config --static "$@"\n' > "$B/pkg-config-static" && chmod +x "$B/pkg-config-static"
PKG_CONFIG="$B/pkg-config-static" LDFLAGS="-Wl,-Bsymbolic -static-libgcc" meson setup "$B/mpv" "$B/mpv-$MPV" --buildtype=release --prefix=$P --libdir=lib \
    -Dlibmpv=true -Dcplayer=false -Dbuild-date=false -Dmanpage-build=disabled -Dhtml-build=disabled \
    -Dlua=disabled -Djavascript=disabled -Dcplugins=disabled -Dlibarchive=disabled -Dlibbluray=disabled \
    -Ddvdnav=disabled -Ddvda=disabled -Dcdda=disabled -Ddvbin=disabled -Dlibcurl=disabled \
    -Duchardet=disabled -Drubberband=disabled -Dlcms2=disabled -Dzimg=disabled -Djpeg=disabled \
    -Dvapoursynth=disabled -Dsubrandr=disabled -Dsdl2-audio=disabled -Dsdl2-video=disabled \
    -Dsdl2-gamepad=disabled -Dx11=disabled -Dx11-clipboard=disabled -Dxv=disabled -Dwayland=disabled \
    -Ddmabuf-wayland=disabled -Dgl=disabled -Dplain-gl=disabled -Degl=disabled -Dvulkan=disabled \
    -Dshaderc=disabled -Dspirv-cross=disabled -Ddrm=disabled -Dgbm=disabled -Dcaca=disabled \
    -Dsixel=disabled -Dvaapi=disabled -Dvdpau=disabled -Dcuda-hwaccel=disabled -Dcuda-interop=disabled \
    -Dpipewire=disabled -Djack=disabled -Dopenal=disabled -Dsndio=disabled -Doss-audio=disabled \
    -Dpulse=enabled -Dalsa=enabled -Dlibavdevice=disabled -Dzlib=enabled > "$B/mpv-setup.log" || { tail -30 "$B/mpv-setup.log"; exit 1; }
quiet ninja -C "$B/mpv" install

OUT=$B/out
mkdir -p "$OUT"
cp -L $P/lib/libmpv.so.2 "$OUT/"
# Libraries a desktop may lack (libass and what it uses); the rest (glibc, freetype, fontconfig,
# PulseAudio, ALSA) every desktop has.
for lib in libass.so.9 libfribidi.so.0 libharfbuzz.so.0 libgraphite2.so.3; do
    cp -L "/usr/lib/x86_64-linux-gnu/$lib" "$OUT/"
done
for f in "$OUT"/*; do strip --strip-unneeded "$f"; patchelf --set-rpath '$ORIGIN' "$f"; done
# Nothing left unresolved from C++ (libstdc++ above; weak symbols, as its transactional memory
# hooks, may stay).
CXX_LEFT=$(nm -D --undefined-only "$OUT/libmpv.so.2" | grep ' U _Z' || true)
[[ -z $CXX_LEFT ]] || { echo "$CXX_LEFT"; exit 1; }
echo "== needed"
for f in "$OUT"/*; do readelf -d "$f" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'; done | sort -u | tr '\n' ' '
echo
cp "$B/mpv-$MPV/LICENSE.GPL" "$OUT/LICENSE.mpv" 2>/dev/null || cp "$B/mpv-$MPV/Copyright" "$OUT/LICENSE.mpv"
cp "$B/ffmpeg-$FFMPEG/COPYING.LGPLv2.1" "$OUT/LICENSE.ffmpeg"
cp "$B/dav1d-$DAV1D/COPYING" "$OUT/LICENSE.dav1d"
cp "$B/libplacebo-src/LICENSE" "$OUT/LICENSE.libplacebo"
cp "$B/libplacebo-src/3rdparty/fast_float/LICENSE-MIT" "$OUT/LICENSE.fast_float"
# The libraries copied from Ubuntu above: their package's copyright file, followed by the full
# text of the common licences it refers to.
for pkg in libass9:libass libfribidi0:fribidi libharfbuzz0b:harfbuzz libgraphite2-3:graphite2; do
    c=/usr/share/doc/${pkg%%:*}/copyright
    { cat "$c"
      for l in $(grep -o '/usr/share/common-licenses/[A-Za-z0-9.+-]*[A-Za-z0-9]' "$c" | sort -u); do
          printf '\n\n===== %s\n\n' "$l"; cat "$l"
      done
    } > "$OUT/LICENSE.${pkg#*:}"
done
NAME=libmpv-linux-x64-$MPV-$REV.tar.gz
tar -C "$OUT" --owner=0 --group=0 --sort=name --mtime=@0 -czf "/src/target/mpv-linux/$NAME" .
ls -la "$OUT" "/src/target/mpv-linux/$NAME"
sha256sum "/src/target/mpv-linux/$NAME"
