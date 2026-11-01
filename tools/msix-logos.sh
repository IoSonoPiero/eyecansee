#!/bin/sh
# The Microsoft Store package's logos (tools/msix/Assets), drawn from assets/eyecansee.svg.
# Start, the taskbar and Settings take Square44x44Logo.targetsize-N_altform-unplated (dark theme) and
# _altform-lightunplated (light theme): without them Windows shrinks the icon onto a grey plate.
# The same picture for all three: the blue square reads on both themes. resources.pri (makepri, in
# tools/msix.ps1) lets Windows find the qualified names. Run again after changing the SVG.
set -e
cd "$(dirname "$0")/.."
out=tools/msix/Assets
rm -rf "$out" && mkdir -p "$out"
png() { magick -background none -density 1536 assets/eyecansee.svg -resize "$1x$1" -strip "PNG32:$out/$2"; }
for n in 16 20 24 30 32 36 40 48 60 64 72 80 96 256; do
    png "$n" "Square44x44Logo.targetsize-$n.png"
    png "$n" "Square44x44Logo.targetsize-${n}_altform-unplated.png"
    png "$n" "Square44x44Logo.targetsize-${n}_altform-lightunplated.png"
done
for s in 100 200 400; do
    png $((44 * s / 100)) "Square44x44Logo.scale-$s.png"
    png $((150 * s / 100)) "Square150x150Logo.scale-$s.png"
    png $((50 * s / 100)) "StoreLogo.scale-$s.png"
done
