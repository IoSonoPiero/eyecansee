#!/usr/bin/env bash
# Warns (never fails) about files in src/ longer than 800 lines, not counting the `#[cfg(test)]`
# module at the end, and about the pictures of the illustrated guide (docs/help/img/*.webp, built
# into the program): each under 250 KB, all together under 3 MB. On GitHub Actions the warning
# shows up as an annotation on the file.
# Usage: tools/check-sizes.sh [limit]
set -euo pipefail
cd "$(dirname "$0")/.."
LIMIT=${1:-800}
for f in src/*.rs; do
    n=$(awk '/^#\[cfg\(test\)\]/ { exit } { n++ } END { print n + 0 }' "$f")
    if (( n > LIMIT )); then
        echo "::warning file=$f::$f has $n lines without tests (limit $LIMIT): split it by topic"
    fi
done
IMG_LIMIT=$((250 * 1024))
IMG_TOTAL_LIMIT=$((3 * 1024 * 1024))
total=0
for f in docs/help/img/*.webp; do
    [[ -e $f ]] || continue
    n=$(stat -c %s "$f")
    total=$((total + n))
    if (( n > IMG_LIMIT )); then
        echo "::warning file=$f::$f is $n bytes (limit $IMG_LIMIT): a smaller part, width or quality"
    fi
done
if (( total > IMG_TOTAL_LIMIT )); then
    echo "::warning::docs/help/img/*.webp are $total bytes together (limit $IMG_TOTAL_LIMIT): the program carries them all"
fi
