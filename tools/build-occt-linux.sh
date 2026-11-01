#!/bin/bash
# The CAD engine, downloaded by EyeCanSee the first time a STEP, IGES or BREP file is opened
# (src/occt.rs, src/component.rs): Open CASCADE Technology built from its official sources,
# only the toolkits for reading STEP and IGES with colours and assemblies (XCAF) and for turning
# surfaces into triangles, linked statically with our C interface (tools/occt/ecs_occt.cpp)
# into one library. Built in the AppImage's container (Ubuntu 22.04, glibc 2.35), with libstdc++
# inside, so that it runs on older distributions too.
# Output: target/occt-linux/occt-linux-x64-<version>-<build>.tar.gz (libecs_occt.so, licences, source).
set -euo pipefail
OCCT=8.0.1 TAG=V8.0.1 BUILD=1

if [[ ! -f /.dockerenv && ! -f /run/.containerenv ]]; then
    mkdir -p target/occt-linux
    exec podman run --rm -v "$PWD":/src:z -v "${APPIMAGE_CACHE:-eyecansee-appimage-cache}":/cache -w /src \
        docker.io/library/ubuntu:22.04 bash tools/build-occt-linux.sh
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential cmake ninja-build curl ca-certificates >/dev/null

B=$(mktemp -d -p /cache)
P=$B/prefix
curl -sSfL "https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/$TAG.tar.gz" | tar xz -C "$B"
SRC=$(echo "$B"/OCCT-*)

# Only what reading needs: no drawing (OpenGL, FreeType, X11, Tk), no other formats' libraries.
cmake -S "$SRC" -B "$B/build" -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_LIBRARY_TYPE=Static \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DCMAKE_CXX_FLAGS="-ffunction-sections -fdata-sections -fvisibility=hidden -fvisibility-inlines-hidden" \
    -DINSTALL_DIR="$P" -DBUILD_MODULE_FoundationClasses=ON -DBUILD_MODULE_ModelingData=ON \
    -DBUILD_MODULE_ModelingAlgorithms=ON -DBUILD_MODULE_DataExchange=OFF -DBUILD_MODULE_Visualization=OFF \
    -DBUILD_MODULE_ApplicationFramework=OFF -DBUILD_MODULE_Draw=OFF \
    -DBUILD_ADDITIONAL_TOOLKITS="TKDE TKXSBase TKDESTEP TKDEIGES TKXCAF TKRWMesh TKService TKV3d TKVCAF TKCDF TKLCAF TKCAF" \
    -DUSE_FREETYPE=OFF -DUSE_XLIB=OFF -DUSE_OPENGL=OFF -DUSE_GLES2=OFF -DUSE_TK=OFF -DUSE_FREEIMAGE=OFF \
    -DUSE_RAPIDJSON=OFF -DUSE_DRACO=OFF -DUSE_TBB=OFF -DUSE_VTK=OFF -DUSE_FFMPEG=OFF -DUSE_OPENVR=OFF \
    -DBUILD_DOC_Overview=OFF > "$B/cmake.log" || { tail -30 "$B/cmake.log"; exit 1; }
ninja -C "$B/build" install > "$B/build.log" || { tail -40 "$B/build.log"; exit 1; }

OUT=$B/out
mkdir -p "$OUT"
g++ -std=c++17 -O2 -fPIC -shared -fvisibility=hidden -fvisibility-inlines-hidden -ffunction-sections -fdata-sections \
    -I"$P/include/opencascade" tools/occt/ecs_occt.cpp \
    -Wl,--start-group "$P"/lib/*.a -Wl,--end-group -Wl,--exclude-libs,ALL -Wl,--gc-sections -Wl,--no-undefined \
    -static-libstdc++ -static-libgcc -lpthread -ldl -o "$OUT/libecs_occt.so"
strip --strip-unneeded "$OUT/libecs_occt.so"
echo "== exported"
nm -D --defined-only "$OUT/libecs_occt.so" | grep ecs_
echo "== needed"
readelf -d "$OUT/libecs_occt.so" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p' | tr '\n' ' '
echo
cp "$SRC/LICENSE_LGPL_21.txt" "$OUT/LICENSE.occt-LGPL-2.1"
cp "$SRC/OCCT_LGPL_EXCEPTION.txt" "$OUT/LICENSE.occt-exception"
cat > "$OUT/SOURCE.txt" <<EOF
Open CASCADE Technology $OCCT, built by EyeCanSee's tools/build-occt-linux.sh from
https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/$TAG.tar.gz
with tools/occt/ecs_occt.cpp (https://github.com/IoSonoPiero/eyecansee).
EOF
NAME=occt-linux-x64-$OCCT-$BUILD.tar.gz
tar -C "$OUT" --owner=0 --group=0 --sort=name --mtime=@0 -czf "/src/target/occt-linux/$NAME" .
ls -la "$OUT" "/src/target/occt-linux/$NAME"
sha256sum "/src/target/occt-linux/$NAME"
rm -rf "$B"
