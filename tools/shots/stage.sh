#!/usr/bin/env bash
# The demo home of tools/shots.sh: $1 = test files (notes/test-files), $2 = work folder, where it
# makes home/demo, disks/<name> (the contents of the made-up USB stick and external disk),
# settings.json and servers.json. Only files free to show (tools/shots/SOURCES.txt); folders with
# neutral names, servers made up (example.com).
set -euo pipefail
FILES=$1; WORK=$2
D=$WORK/home/demo
H=$D/Pictures/Holiday
mkdir -p "$D"/{Desktop,Downloads,Music,Videos,.cache,.local/share} "$H/Mountains" "$H/Flowers" \
    "$D/Pictures/Phone backup" "$D/Documents/3D models" "$D/Documents/Drawings"
P=$FILES/foto-video
# The 13 photos (not "Ocean waves": no known source), six in two subfolders, and two videos in
# Holiday; three videos in Videos.
for f in "$P"/*.jpg; do cp "$f" "$H/"; done
mv "$H/Denali.jpg" "$H/North Cascades.jpg" "$H/Wilder Kaiser.jpg" "$H/Mountains/"
mv "$H/Apple blossoms.jpg" "$H/Orchid.jpg" "$H/Flowers in a vase.jpg" "$H/Flowers/"
for v in "Bees on flowers" "Flowers" "Earth from space"; do cp "$P/$v.webm" "$D/Videos/"; done
cp "$P/Bees on flowers.webm" "$P/Flowers.webm" "$H/"
# Copies with other names, for Find duplicates: three groups.
n=1
for f in Moth Iceberg Sunset; do cp "$P/$f.jpg" "$D/Pictures/Phone backup/IMG_000$n.jpg"; n=$((n + 1)); done
# The USB stick and the external disk (mounted in the namespace by tools/shots.sh).
S="$WORK/disks/USB STICK"
mkdir -p "$S/Photos" "$S/Documents" "$WORK/disks/BACKUP/Pictures" "$WORK/disks/BACKUP/Videos"
n=101
for f in "Lake Sherburne" "North Cascades" "Grand Canyon" "Wilder Kaiser" "Apple blossoms" "Quebec City" Iceberg Moth; do
    cp "$P/$f.jpg" "$S/Photos/IMG_0$n.jpg"; n=$((n + 1))
done
for m in Avocado Lantern ToyCar; do cp "$FILES/3d/gltf/$m.glb" "$D/Documents/3D models/"; done
# A flat plan of ours (tools/shots/Flat.dxf); the cad scene saves it as DWG with Export drawing.
cp "$(dirname "$0")/Flat.dxf" "$D/Documents/Drawings/"
# A photo with its real place for the metadata panel: a phone's raw file from raw.pixls.us (CC0),
# above Lake Garda, with the GPS position, altitude and date the phone wrote.
cp "$FILES/formati/RAW/Google - Pixel 6 Pro - 16bit (4-3).dng" "$H/Lake Garda.dng"
# One date for all, as if copied from the camera after a holiday (the copies a day later).
find "$D" "$WORK/disks" -type f -exec touch -d "2025-07-14 18:30" {} +
find "$D/Pictures/Phone backup" -type f -exec touch -d "2025-07-15 09:12" {} +
cat > "$WORK/settings.json" <<'JSON'
{"check_updates": false, "theme": "dark", "remember_location": false, "window_start": "normal", "meta_collapsed": true,
 "bookmarks": [{"name": "Holiday", "address": "/home/demo/Pictures/Holiday"},
               {"name": "3D models", "address": "/home/demo/Documents/3D models"},
               {"name": "Photo archive", "address": "sftp://demo@photos.example.com/Pictures/"}]}
JSON
cat > "$WORK/servers.json" <<'JSON'
[{"name": "Photo archive", "scheme": "sftp", "host": "photos.example.com", "port": 22, "user": "demo", "folder": "Pictures"},
 {"name": "Home NAS", "scheme": "ftp", "host": "nas.example.com", "user": "demo", "folder": ""}]
JSON
