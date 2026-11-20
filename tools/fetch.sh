# Sourced by the build scripts (build-appimage.sh, build-mpv-linux.sh, build-occt-linux.sh).
# fetch <url> <sha256> <file>: downloads <url> to <file> and checks its SHA-256; on a mismatch the
# script stops and nothing is left under <file>. Every source a build script downloads goes through
# here, with its SHA-256 written next to its version: a changed archive never reaches the compiler.
fetch() {
    # Explicit exits: set -e does not apply inside a function called from a && list.
    curl -sSfL -o "$3.part" "$1" || { rm -f "$3.part"; echo "$1: download failed" >&2; exit 1; }
    echo "$2  $3.part" | sha256sum -c --quiet || { rm -f "$3.part"; echo "$1: SHA-256 mismatch" >&2; exit 1; }
    mv "$3.part" "$3"
}
