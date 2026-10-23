#!/usr/bin/env bash
# Signs the files of a draft release with the developer's minisign key and uploads the signatures
# (<file>.minisig) next to them: EyeCanSee installs an update only with a valid signature from a key
# in PUBLIC_KEYS (src/update.rs). Run it on the developer's PC once the workflow has made the draft;
# the release is then checked and published by hand (this script never publishes it).
# Usage: tools/sign-release.sh v<version>
# Needs minisign (Fedora: sudo dnf install minisign) and gh. The private key stays on this PC:
# ~/.minisign/eyecansee.key, or the file in EYECANSEE_SIGN_KEY.
#
# The key pair is made once:
#   mkdir -p ~/.minisign && minisign -G -p ~/.minisign/eyecansee.pub -s ~/.minisign/eyecansee.key
# minisign asks for a password that protects the private key. The second line of eyecansee.pub is
# the public key that goes in PUBLIC_KEYS and in the README.
# ⚠️ Keep an encrypted backup of eyecansee.key (and its password) off this PC: without it no update
# can be signed any more, and users would have to update by hand to a version with a new key.
set -euo pipefail

TAG=${1:?usage: tools/sign-release.sh v<version>}
VER=${TAG#v}
KEY=${EYECANSEE_SIGN_KEY:-$HOME/.minisign/eyecansee.key}
REPO=IoSonoPiero/eyecansee
[[ -f $KEY ]] || { echo "no private key in $KEY" >&2; exit 1; }

DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT
gh release download "$TAG" --repo "$REPO" --dir "$DIR" --pattern 'EyeCanSee-*'
# Signatures of an earlier run are made again.
rm -f "$DIR"/*.minisig
NOTES=$(gh release view "$TAG" --repo "$REPO" --json body --jq .body)

cd "$DIR"
for f in EyeCanSee-*; do
    # The SHA-256 the workflow wrote in the draft notes when it built the file.
    want=$(grep -oP "^SHA-256 of \Q$f\E: \`\K[0-9a-f]{64}" <<< "$NOTES" || true)
    [[ -n $want ]] || { echo "no SHA-256 for $f in the notes of $TAG" >&2; exit 1; }
    got=$(sha256sum "$f" | cut -d' ' -f1)
    [[ $got == "$want" ]] || { echo "$f: SHA-256 $got, the notes say $want" >&2; exit 1; }
    echo "Signing $f"
    minisign -S -s "$KEY" -m "$f" -x "$f.minisig" -t "file:$f version:$VER"
done
gh release upload "$TAG" --repo "$REPO" --clobber ./*.minisig
echo "Signatures uploaded to $TAG. The release is still to be checked and published by hand."
