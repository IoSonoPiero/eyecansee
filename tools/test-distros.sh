#!/usr/bin/env bash
# Starts the AppImage in podman containers of other distributions, on the host's Wayland, with a
# temporary configuration. For each distribution: "bare" (base image, shows which libraries the
# AppImage expects from the system) and "desktop" (with the graphics libraries a desktop has).
# EXIT=124 means it was still running when stopped after 12 s, i.e. it started.
# Usage: tools/test-distros.sh <AppImage> <results folder> [distro...]
set -u
APP=$(realpath "$1"); OUT=$(realpath -m "$2"); shift 2
mkdir -p "$OUT"
declare -A IMG=(
    [ubuntu22]=docker.io/library/ubuntu:22.04
    [ubuntu24]=docker.io/library/ubuntu:24.04
    [debian12]=docker.io/library/debian:12
    [debian13]=docker.io/library/debian:13
    [opensuse]=registry.opensuse.org/opensuse/tumbleweed
    [arch]=docker.io/library/archlinux
    [fedora44]=registry.fedoraproject.org/fedora:44
)
apt='apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq libegl1 libgl1 libgl1-mesa-dri libwayland-client0 libwayland-egl1 libxkbcommon0 libfreetype6 zlib1g >/dev/null'
declare -A PKG=(
    [ubuntu22]=$apt [ubuntu24]=$apt [debian12]=$apt [debian13]=$apt
    [opensuse]='zypper -q -n in Mesa-libEGL1 Mesa-dri libwayland-client0 libwayland-egl1 libxkbcommon0 libfreetype6 libz1 >/dev/null'
    [arch]='pacman -Sy --noconfirm --needed mesa libglvnd wayland libxkbcommon freetype2 zlib >/dev/null'
    [fedora44]='dnf -q -y install mesa-libEGL mesa-dri-drivers libwayland-client libwayland-egl libxkbcommon freetype zlib >/dev/null'
)
# No FUSE in a container: the AppImage is extracted and its AppRun started.
run='export XDG_RUNTIME_DIR=/run/xdg WAYLAND_DISPLAY=wayland-0 HOME=/tmp/h XDG_CONFIG_HOME=/tmp/h/c XDG_CACHE_HOME=/tmp/h/k EYECANSEE_LOG=1 RUST_BACKTRACE=1
mkdir -p $HOME; cd /tmp; /app/eyecansee.AppImage --appimage-extract >/dev/null 2>&1
grep -h PRETTY_NAME /etc/os-release; ldd --version | head -1
ldd squashfs-root/usr/bin/eyecansee | grep "not found"
LD_LIBRARY_PATH=squashfs-root/usr/lib timeout 12 squashfs-root/AppRun 2>&1 | head -60; echo "EXIT=${PIPESTATUS[0]}"'
for d in "${@:-${!IMG[@]}}"; do
    for mode in bare desktop; do
        pre=true; [ $mode = desktop ] && pre=${PKG[$d]}
        echo "== $d $mode"
        podman run --rm --security-opt label=disable --device /dev/dri \
            -v "$APP:/app/eyecansee.AppImage:ro" -v "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY:/run/xdg/wayland-0" \
            "${IMG[$d]}" bash -c "$pre; $run" > "$OUT/$d-$mode.log" 2>&1
        grep -E "PRETTY|EXIT=|not found|panick|rror" "$OUT/$d-$mode.log" | head -6
    done
done
