#!/usr/bin/env bash
# Takes again the pictures of the illustrated guide, docs/help/img/<name>.webp: all the scenes of
# tools/shots/, or those named (tools/shots.sh start editor). EyeCanSee, built with the feature
# `shots`, plays each scene (src/shots.rs) and saves the pictures itself.
#
# Where: a desktop session with the screen on and not locked (Wayland sends no frames to a window
# not shown: the picture would come out empty); the window opens on the screen for each scene.
# What it needs: unshare, jq, and the test files of the private notes (notes/test-files; another
# folder with EYECANSEE_SHOTS_FILES), of which it copies only the freely licensed ones listed in
# tools/shots/SOURCES.txt.
#
# Isolation, as for the announcement video: a user, mount and network namespace with an empty home
# (/home/demo) made for the pictures, settings in it (dark theme, English, no update check), made-up
# servers (example.com), a made-up USB stick and external disk (/run/media/demo), and the real disks
# hidden (/sys/class/block, /dev/disk/by-label, /run/media, /mnt, gvfs), as the real USB devices
# but in the scenes of a phone. Nothing of the user's settings, cache or disks is read or written.
#
# The site's pictures (site/img, docs/screenshots) have their scenes in tools/shots/site/, left out
# when no scene is named: tools/shots.sh site/guide site/guide-search site/bookmarks
# site/open-location site/servers, with EYECANSEE_SHOTS_TO=<folder> for the pictures (default
# docs/help/img). site/grid, site/viewer, site/devices, site/video-convert and site/model-3d are those of the
# README too (docs/screenshots/<name>.png, the first command below only); grid and viewer want a
# real phone with the folder DCIM/EyeCanSee-demo (docs/screenshots/SOURCES.txt), devices the FTP
# server below.
# The tips at startup (src/tips.rs) have their own pictures, docs/tips/<name>.webp, from the scenes of
# tools/shots/tips/, also left out when no scene is named: EYECANSEE_SHOTS_TO=docs/tips tools/shots.sh
# tips/help tips/right-click tips/cover tips/details tips/metadata tips/slideshow.
# Each is the whole window, 1200 x 800 (for those of 0.76.0 src/shots.rs wrote a lossless WebP,
# `encode_lossless()` in place of `encode(QUALITY)`, changed for that run only); then, with
# ImageMagick, for site/img/<name>.png and .webp and docs/screenshots/<name>.webp:
#   magick <name>.webp -strip <name>.png
#   magick <name>.png -resize x533 -strip -quality 82 -define webp:method=6 site/img/<name>.webp
#   magick <name>.png -strip -quality 82 -define webp:method=6 docs/screenshots/<name>.webp
# (docs/screenshots/servers.png stays a PNG, as the README shows it: the first command only.)
#
# A scene with the line `ftp-server` has a real FTP server behind the made-up "Home NAS"
# (nas.example.com, user demo, the 13 photos in Photos), started in the namespace for that scene
# only: it needs pyftpdlib, taken from the Python of EYECANSEE_SHOTS_PYTHON (default python3), for
# example from a venv made for it:
#   python3 -m venv /tmp/shots-ftp && /tmp/shots-ftp/bin/pip install pyftpdlib
#   EYECANSEE_SHOTS_PYTHON=/tmp/shots-ftp/bin/python tools/shots.sh site/devices
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)

if [[ ${1:-} == --inside ]]; then
    # In the namespace: $2 = work folder (outside /home), then the scenes.
    WORK=$2; shift 2
    ip link set lo up 2>/dev/null || true
    mount --bind "$WORK/home" /home
    # Of the made-up servers only nas.example.com has an address, for the FTP server of `ftp-server`.
    printf '127.0.0.1 localhost nas.example.com\n' > "$WORK/hosts"
    mount --bind "$WORK/hosts" /etc/hosts
    for d in /sys/class/block /dev/disk/by-label /run/media /mnt "/run/user/$SHOTS_UID/gvfs"; do
        if [[ -d $d ]]; then mount -t tmpfs tmpfs "$d"; fi
    done
    # A USB stick and an external disk, made up: a tmpfs mounted from /dev/sdX1, with a
    # /sys/class/block/sdX1 that leads to a path with "/usb" in it, is what the app takes for an
    # external disk (devices.rs, is_external); the name is that of the mount point.
    for disk in "sdb1:USB STICK" "sdc1:BACKUP"; do
        dev=${disk%%:*}; label=${disk#*:}
        mkdir -p "/sys/class/block/devices/usb1/1-1/$dev" "/run/media/demo/$label"
        ln -s "devices/usb1/1-1/$dev" "/sys/class/block/$dev"
        mount -t tmpfs "/dev/$dev" "/run/media/demo/$label"
        # With their dates (stage.sh): a plain copy would date the files today.
        cp -r --preserve=timestamps "$WORK/disks/$label/." "/run/media/demo/$label/"
    done
    for scene in "$@"; do
        name=$(basename "$scene" .txt)
        # The real USB devices (a phone connected to the computer) stay out of the pictures, but
        # in the scenes of a phone (`phone-folders`): their list in /sys hidden.
        usb=/sys/bus/usb/devices
        if grep -q '^phone-folders ' "$scene"; then
            if mountpoint -q "$usb"; then umount "$usb"; fi
        elif ! mountpoint -q "$usb"; then
            mount -t tmpfs tmpfs "$usb"
        fi
        conf=/home/demo/.config/eyecansee
        rm -rf "$conf" && mkdir -p "$conf"
        # Settings: the common ones, then the scene's `set <key> <json>` lines.
        settings=$(cat "$WORK/settings.json")
        while read -r _ key value; do
            settings=$(jq -c --arg k "$key" --argjson v "$value" '.[$k] = $v' <<<"$settings")
        done < <(grep -E '^set ' "$scene" || true)
        echo "$settings" > "$conf/settings.json"
        install -m 600 "$WORK/servers.json" "$conf/servers.json"
        ftp=
        if grep -q '^ftp-server' "$scene"; then
            # The saved FTP server with its password, so that it opens without asking.
            jq '(.[] | select(.scheme == "ftp")).password = "demo"' "$WORK/servers.json" > "$conf/servers.json"
            "${EYECANSEE_SHOTS_PYTHON:-python3}" -m pyftpdlib -i 127.0.0.1 -p 21 -d "$WORK/ftp" -u demo -P demo \
                2>"$WORK/log/$name-ftp.log" &
            ftp=$!
            sleep 1
        fi
        open=$(sed -n 's/^open  *//p' "$scene" | head -1)
        echo "$name: ${open:-/home/demo/Pictures}" >&2
        (cd /home/demo && env HOME=/home/demo XDG_CONFIG_HOME=/home/demo/.config XDG_CACHE_HOME=/home/demo/.cache \
            XDG_DATA_HOME=/home/demo/.local/share EYECANSEE_LANG=en EYECANSEE_LOG="$WORK/log/$name.log" \
            EYECANSEE_SHOTS_SCENE="$scene" EYECANSEE_SHOTS_OUT="$WORK/out" \
            timeout 180 "$WORK/eyecansee" "${open:-/home/demo/Pictures}") || echo "$name: exit $?" >&2
        if [[ -n $ftp ]]; then kill "$ftp"; wait "$ftp" 2>/dev/null || true; fi
    done
    exit 0
fi

FILES=${EYECANSEE_SHOTS_FILES:-$ROOT/notes/test-files}
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}
SCENES=()
for n in "${@:-}"; do
    [[ -z $n ]] && continue
    [[ -f $ROOT/tools/shots/$n.txt ]] || { echo "no scene tools/shots/$n.txt" >&2; exit 1; }
    SCENES+=("$n")
done
if (( ${#SCENES[@]} == 0 )); then
    for f in "$ROOT"/tools/shots/*.txt; do
        [[ $(basename "$f") == SOURCES.txt ]] || SCENES+=("$(basename "$f" .txt)")
    done
fi
[[ -d $FILES/foto-video ]] || { echo "test files not found in $FILES (EYECANSEE_SHOTS_FILES)" >&2; exit 1; }
for n in "${SCENES[@]}"; do
    if grep -q '^ftp-server' "$ROOT/tools/shots/$n.txt" && ! "${EYECANSEE_SHOTS_PYTHON:-python3}" -c 'import pyftpdlib' 2>/dev/null; then
        echo "$n needs pyftpdlib: see the top of this script (EYECANSEE_SHOTS_PYTHON)" >&2; exit 1
    fi
done
if [[ $(gdbus call --session --dest org.gnome.ScreenSaver --object-path /org/gnome/ScreenSaver \
        --method org.gnome.ScreenSaver.GetActive 2>/dev/null) == "(true,)" ]]; then
    echo "the screen is locked or off: the pictures would come out empty" >&2; exit 1
fi

(cd "$ROOT" && cargo build --release --features shots)
# Outside /home, which the namespace replaces with the demo home.
WORK=$(mktemp -d "${TMPDIR:-/tmp}/eyecansee-shots.XXXXXX")
[[ $WORK != /home/* ]] || { echo "TMPDIR must be outside /home" >&2; exit 1; }
# EYECANSEE_SHOTS_KEEP=1 keeps it, with the logs of each scene in log/.
[[ -n ${EYECANSEE_SHOTS_KEEP:-} ]] && echo "work folder: $WORK" >&2 || trap 'rm -rf "$WORK"' EXIT
cp "$TARGET/release/eyecansee" "$WORK/"
mkdir -p "$WORK/scenes" "$WORK/out" "$WORK/log"
PATHS=()
for n in "${SCENES[@]}"; do
    mkdir -p "$(dirname "$WORK/scenes/$n.txt")"
    cp "$ROOT/tools/shots/$n.txt" "$WORK/scenes/$n.txt" && PATHS+=("$WORK/scenes/$n.txt")
done
"$ROOT/tools/shots/stage.sh" "$FILES" "$WORK"

# Kept on while the pictures are taken: a screen gone dark sends no frames.
INHIBIT=()
command -v gnome-session-inhibit >/dev/null && INHIBIT=(gnome-session-inhibit --inhibit idle --reason "EyeCanSee screenshots")
"${INHIBIT[@]}" env SHOTS_UID="$(id -u)" unshare -r -n -m --propagation private \
    "$0" --inside "$WORK" "${PATHS[@]}"

TO=${EYECANSEE_SHOTS_TO:-$ROOT/docs/help/img}
mkdir -p "$TO"
missing=0
for n in "${SCENES[@]}"; do
    # A scene can save more pictures than its own (crops): every `shot <name>`.
    for pic in $(awk '$2 == "shot" { print ($3 ~ /^[a-z]/ ? $3 : FILENAME) }' "$WORK/scenes/$n.txt" | sed 's|.*/||; s|\.txt$||' | sort -u); do
        if [[ -f $WORK/out/$pic.webp ]]; then
            cp "$WORK/out/$pic.webp" "$TO/"
            echo "$pic.webp $(stat -c %s "$WORK/out/$pic.webp") B"
        else
            echo "$pic.webp: NOT TAKEN (log: $(tail -3 "$WORK/log/$n.log" 2>/dev/null | tr '\n' ' '))" >&2
            missing=1
        fi
    done
done
"$ROOT/tools/check-sizes.sh"
exit $missing
