#!/usr/bin/env bash
# Builds the private test copy (red icon, its own folders, no updates: src/test_copy.rs) from the
# current checkout and installs it for the current user next to the published EyeCanSee:
# program in ~/.local/opt/eyecansee-prova (with BUILD.txt: commit and date), menu entry
# "EyeCanSee prova". It uses the system's libheif and ImageMagick, like `cargo build`.
# Uninstall: tools/test-copy/install-linux.sh --uninstall (settings and thumbnails stay in
# ~/.config/eyecansee-prova, ~/.cache/eyecansee-prova, ~/.local/share/eyecansee-prova).
set -euo pipefail
cd "$(dirname "$0")/../.."

DIR="$HOME/.local/opt/eyecansee-prova"
DESKTOP="$HOME/.local/share/applications/eyecansee-prova.desktop"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -rf "$DIR" "$DESKTOP"
else
    cargo build --locked --profile prova --features test-copy
    commit=$(git rev-parse --short HEAD)
    git diff --quiet HEAD || commit="$commit (with uncommitted changes)"
    install -Dm755 target/prova/eyecansee "$DIR/eyecansee"
    install -Dm644 assets/eyecansee-prova.png "$DIR/eyecansee-prova.png"
    printf 'EyeCanSee test copy\nVersion: %s\nBranch: %s\nCommit: %s\nBuilt: %s\n' \
        "$(cat VERSION)" "$(git branch --show-current)" "$commit" "$(date '+%Y-%m-%d %H:%M %Z')" > "$DIR/BUILD.txt"
    # No MimeType: the test copy never becomes the default viewer. StartupWMClass = app id (test_copy::NAME).
    mkdir -p "$(dirname "$DESKTOP")"
    cat > "$DESKTOP" <<DESK
[Desktop Entry]
Type=Application
Name=EyeCanSee prova
Comment=Private test copy of EyeCanSee
Exec=$DIR/eyecansee %f
Icon=$DIR/eyecansee-prova.png
StartupWMClass=eyecansee-prova
Categories=Graphics;Viewer;
Terminal=false
DESK
fi
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "Done."
