#!/usr/bin/env bash
# Tests of "Save" (Image > Transform) with the real code (`loader::tests::save_live_*`), on files the
# tests generate: JPEG 1024x768 and 1001x751, PNG, animated WebP, APNG, two-page TIFF, JPEG with EXIF
# orientation 6 and GPS, JPEG followed by a video as a Motion Photo is. Linux (Fedora), no sudo.
#
#   tools/test-save.sh                  local disk, with the real system trash (only the test's own
#                                       items are taken out of it again)
#   tools/test-save.sh <test>...        only those tests (e.g. save_live_lossless)
#   EYECANSEE_SAVE_DIR=<folder on a stick> tools/test-save.sh     also on the stick (exFAT)
#   EYECANSEE_MTP=<serial> tools/test-save.sh                     also on a phone (EyeCanSee closed)
#
# The files stay in a new folder in the home (same disk as the home trash), printed at the end with
# the log of each test.
set -u
cd "$(dirname "$0")/.."

out=$(mktemp -d "$HOME/eyecansee-test-save.XXXXXX")
tmp=$(mktemp -d)
# Never the real settings and thumbnails.
export EYECANSEE_SAVE_OUT="$out" XDG_CONFIG_HOME="$tmp/config" XDG_CACHE_HOME="$tmp/cache"
build=(--release --no-default-features --features heif,magick)

if (($#)); then
    tests=("$@")
else
    tests=(save_live_trash save_live_lossless save_live_excluded save_live_readonly save_live_recover)
    [[ -n "${EYECANSEE_SAVE_DIR:-}" ]] && tests+=(save_live_dir)
    [[ -n "${EYECANSEE_MTP:-}" ]] && tests+=(mtp_save_live)
fi

cargo test "${build[@]}" --no-run -q || { echo "Build failed"; exit 1; }
declare -A result
for t in "${tests[@]}"; do
    echo "== $t"
    # A wrong name runs no test and still exits 0: "1 passed" is what counts.
    if cargo test "${build[@]}" -q -- --ignored --exact "loader::tests::$t" --nocapture >"$out/$t.log" 2>&1 \
        && grep -q "1 passed" "$out/$t.log"; then
        result[$t]=OK
    else
        result[$t]=FAILED
        grep -E "panicked|assert|left:|right:" "$out/$t.log"
    fi
done
rm -rf "$tmp"

echo
echo "Summary:"
for t in "${tests[@]}"; do
    printf '  %-20s %s\n' "$t" "${result[$t]}"
done
echo "Files and logs: $out"
[[ ! " ${result[*]} " =~ " FAILED " ]]
