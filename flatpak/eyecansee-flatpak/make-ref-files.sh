#!/bin/bash
# site/eyecansee.flatpakref and site/eyecansee.flatpakrepo with the public key inside, and the key
# itself as site/eyecansee-flatpak.gpg, and the 128 px icon as site/eyecansee.png (Discover and the
# Mint Software Manager show an app opened from a .flatpakref with its Icon=).
#   make-ref-files.sh <public key (gpg --export)> <repository url>
set -euo pipefail
KEY=$(realpath "${1:?public key file}"); URL=${2:?url of the OSTree repository, ending in /}
ICON=$(realpath "${ICON:-$(dirname "$0")/../../assets/eyecansee.png}")
cd "$(dirname "$0")/site"
magick "$ICON" -resize 128x128 eyecansee.png
SITE=${URL%repo/}
B64=$(base64 -w0 "$KEY")
cp "$KEY" eyecansee-flatpak.gpg
cat > eyecansee.flatpakref <<REF
[Flatpak Ref]
Title=EyeCanSee
Name=io.github.IoSonoPiero.EyeCanSee
Branch=master
Url=$URL
SuggestRemoteName=eyecansee
Homepage=https://github.com/IoSonoPiero/eyecansee
IsRuntime=false
Icon=${SITE}eyecansee.png
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
GPGKey=$B64
REF
cat > eyecansee.flatpakrepo <<REPO
[Flatpak Repo]
Title=EyeCanSee
Url=$URL
Homepage=https://github.com/IoSonoPiero/eyecansee
Comment=Image and video viewer and manager
Icon=${SITE}eyecansee.png
GPGKey=$B64
REPO
