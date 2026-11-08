#!/bin/bash
# site/eyecansee.flatpakref and site/eyecansee.flatpakrepo with the public key inside, and the key
# itself as site/eyecansee-flatpak.gpg.  make-ref-files.sh <public key (gpg --export)> <repository url>
# No Icon=: GNOME Software then showed a generic icon instead of the AppStream one, and Discover and
# the Mint Software Manager ignored it (tried on 2026-11-08).
set -euo pipefail
KEY=$(realpath "${1:?public key file}"); URL=${2:?url of the OSTree repository, ending in /}
cd "$(dirname "$0")/site"
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
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
GPGKey=$B64
REF
cat > eyecansee.flatpakrepo <<REPO
[Flatpak Repo]
Title=EyeCanSee
Url=$URL
Homepage=https://github.com/IoSonoPiero/eyecansee
Comment=Image and video viewer and manager
GPGKey=$B64
REPO
