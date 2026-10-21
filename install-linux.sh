#!/usr/bin/env bash
# Builds EyeCanSee and installs it for the current user (no sudo).
# Run it again after every code update. Uninstall: ./install-linux.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")"

BIN="$HOME/.local/bin/eyecansee"
DESKTOP="$HOME/.local/share/applications/eyecansee.desktop"
ICON="$HOME/.local/share/icons/hicolor/scalable/apps/eyecansee.svg"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "$BIN" "$DESKTOP" "$ICON"
else
    cargo build --release
    install -Dm755 target/release/eyecansee "$BIN"
    install -Dm644 assets/eyecansee.desktop "$DESKTOP"
    install -Dm644 assets/eyecansee.svg "$ICON"
fi
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Done."
