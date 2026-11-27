#!/usr/bin/env bash
# Builds the private test copy (red icon, its own folders, no updates: src/test_copy.rs) from the
# current checkout and installs it for the current user next to the published EyeCanSee:
# program in ~/.local/opt/eyecansee-prova (with BUILD.txt: commit and date), menu entry
# "EyeCanSee prova". It uses the system's libheif and ImageMagick, like `cargo build`.
# The menu entry starts it through avvia.sh, which always writes the log (Piero, 21/11/2026: a crash
# during the tests must leave its trace): one file per start in ~/.cache/eyecansee-prova/registri,
# with the program's own log, its standard error (a panic of the main thread) and backtraces; the
# last 30 are kept.
# Uninstall: tools/test-copy/install-linux.sh --uninstall (settings and thumbnails stay in
# ~/.config/eyecansee-prova, ~/.cache/eyecansee-prova, ~/.local/share/eyecansee-prova).
set -euo pipefail
cd "$(dirname "$0")/../.."

DIR="$HOME/.local/opt/eyecansee-prova"
DESKTOP="$HOME/.local/share/applications/eyecansee-prova.desktop"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -rf "$DIR" "$DESKTOP"
else
    # When it was built, shown next to the version (test_copy::BUILT).
    EYECANSEE_BUILT="$(date '+%Y-%m-%d %H:%M:%S')" cargo build --locked --profile prova --features test-copy
    commit=$(git rev-parse --short HEAD)
    git diff --quiet HEAD || commit="$commit (with uncommitted changes)"
    install -Dm755 target/prova/eyecansee "$DIR/eyecansee"
    install -Dm644 assets/eyecansee-prova.png "$DIR/eyecansee-prova.png"
    # The licences of the libraries next to the program, as in the zip: About opens them (src/licenses.rs).
    # Not needed to start it: without jq, About only has no link.
    tools/third-party-licenses.sh "$DIR/THIRD-PARTY-LICENSES.txt" --locked --features test-copy 2>/dev/null \
        || echo "THIRD-PARTY-LICENSES.txt not made (is jq installed?)"
    cat > "$DIR/avvia.sh" <<'RUN'
#!/bin/sh
logs="$HOME/.cache/eyecansee-prova/registri"
mkdir -p "$logs"
ls -1t "$logs"/registro-*.log "$logs"/stderr-*.log 2>/dev/null | tail -n +30 | xargs -r rm -f
# The diagnostic log is the program's own, switched on in Options > Diagnostics (Piero, 23/11/2026:
# "let me switch it on myself, and it stays on"); no EYECANSEE_LOG, which would lock the switch.
# What the program and its libraries write to stderr (a crash's backtrace, mpv) still goes here.
log="$logs/stderr-$(date +%Y-%m-%d_%H%M%S).log"
RUST_BACKTRACE=1 exec "$(dirname "$0")/eyecansee" "$@" 2>> "$log"
RUN
    chmod 755 "$DIR/avvia.sh"
    printf 'EyeCanSee test copy\nVersion: %s\nBranch: %s\nCommit: %s\nBuilt: %s\n' \
        "$(cat VERSION)" "$(git branch --show-current)" "$commit" "$(date '+%Y-%m-%d %H:%M %Z')" > "$DIR/BUILD.txt"
    # The types of the published app, so that "Open with" offers the test copy (Piero, 23/11/2026);
    # not folders, which stay with the file manager. It becomes the default only when chosen.
    # StartupWMClass = app id (test_copy::NAME).
    mime=$(grep '^MimeType=' assets/eyecansee.desktop | sed 's|inode/directory;||')
    mkdir -p "$(dirname "$DESKTOP")"
    cat > "$DESKTOP" <<DESK
[Desktop Entry]
Type=Application
Name=EyeCanSee prova
Comment=Private test copy of EyeCanSee
Exec=$DIR/avvia.sh %f
Icon=$DIR/eyecansee-prova.png
StartupWMClass=eyecansee-prova
Categories=Graphics;Viewer;
$mime
Terminal=false
DESK
fi
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "Done."
