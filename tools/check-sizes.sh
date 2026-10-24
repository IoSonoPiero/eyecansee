#!/usr/bin/env bash
# Warns (never fails) about files in src/ longer than 800 lines, not counting the `#[cfg(test)]`
# module at the end. On GitHub Actions the warning shows up as an annotation on the file.
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
