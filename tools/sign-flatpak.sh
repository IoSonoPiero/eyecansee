#!/bin/bash
# Signs the Flatpak of a release with the repository's GPG key and puts it, as a draft, in a release
# of IoSonoPiero/eyecansee-flatpak; publishing that draft deploys it to Pages (its workflow).
#   tools/sign-flatpak.sh v0.74.2
# Run by the developer in a normal terminal (not inside Claude Code): gpg asks for the passphrase.
# The key never goes to GitHub. Needs gh, ostree, flatpak, gpg.
#  1. flatpak-commit-<ver>.tar from the draft of IoSonoPiero/eyecansee (built by release.yml), checked
#     against the SHA-256 in its notes;
#  2. the published repository: eyecansee-flatpak-repo.tar of the latest published release of
#     IoSonoPiero/eyecansee-flatpak, checked the same way (nothing yet: a new repository);
#  3. the new commit, signed, as the child of the published one (same content as the CI's, checked);
#  4. delta from the published version with the binary inside (21 MB instead of 31 for an update);
#  5. signed summary and appstream, deltas, prune to two versions, deltas of removed versions deleted;
#  6. signatures checked with flatpak/eyecansee-flatpak.gpg, size checked, packed, uploaded as a draft.
# For trials: CI_TAR=<file> PREV_TAR=<file|none> OUT_DIR=<folder> (no download, no upload) and
# PUB=<public key> FLATPAK_GPG_KEY=<id> GNUPGHOME=<folder>.
set -euo pipefail
TAG=${1:?usage: tools/sign-flatpak.sh v<version>}
VER=${TAG#v}
SRC=IoSonoPiero/eyecansee SITE=IoSonoPiero/eyecansee-flatpak
APP=app/io.github.IoSonoPiero.EyeCanSee/x86_64/master
KEY=${FLATPAK_GPG_KEY:-EyeCanSee Flatpak repository}
GPGDIR=${GNUPGHOME:-$HOME/.gnupg}
PUB=${PUB:-$(cd "$(dirname "$0")/.." && pwd)/flatpak/eyecansee-flatpak.gpg}
MAX_MB=200  # two versions and their deltas are ~84 MB; Pages allows 1 GB
die() { echo "sign-flatpak: $*" >&2; exit 1; }
# The file from a release, checked against "SHA-256 of <file>: `<hash>`" in its notes.
fetch() { # repo tag-or-empty file
    local notes want
    gh release download ${2:+"$2"} --repo "$1" --pattern "$3"
    notes=$(gh release view ${2:+"$2"} --repo "$1" --json body --jq .body)
    want=$(grep -oP "^SHA-256 of \Q$3\E: \`\K[0-9a-f]{64}" <<< "$notes") || die "no SHA-256 for $3 in $1 ${2:-latest}"
    echo "$want  $3" | sha256sum -c --quiet || die "$3: SHA-256 does not match"
}
content() { ostree --repo="$1" show "$2" | sed -n 's/^ContentChecksum: *//p'; }

W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
cd "$W"
if [[ -n ${CI_TAR:-} ]]; then cp "$CI_TAR" ci.tar; else fetch "$SRC" "$TAG" "flatpak-commit-$VER.tar"; mv "flatpak-commit-$VER.tar" ci.tar; fi
mkdir ci repo && tar -C ci -xf ci.tar
if [[ -n ${PREV_TAR:-} ]]; then
    [[ $PREV_TAR == none ]] || tar -C repo -xf "$PREV_TAR"
elif gh release view --repo "$SITE" >/dev/null 2>&1; then
    fetch "$SITE" "" eyecansee-flatpak-repo.tar && tar -C repo -xf eyecansee-flatpak-repo.tar && rm eyecansee-flatpak-repo.tar
fi
[[ -f repo/config ]] || ostree init --repo=repo --mode=archive-z2
OLD=$(ostree --repo=repo rev-parse "$APP" 2>/dev/null || true)

flatpak build-commit-from --src-repo=ci --gpg-sign="$KEY" --gpg-homedir="$GPGDIR" --no-update-summary repo "$APP"
NEW=$(ostree --repo=repo rev-parse "$APP")
[[ $NEW != "$OLD" ]] || die "nothing new: the CI's content is the one already published"
[[ $(content repo "$NEW") == "$(content ci "$APP")" ]] || die "the signed commit differs from the CI's"
[[ -z $OLD ]] || ostree --repo=repo static-delta generate --min-fallback-size=200 --from="$OLD" --to="$NEW" >/dev/null
update() { flatpak build-update-repo --gpg-sign="$KEY" --gpg-homedir="$GPGDIR" --title=EyeCanSee "$@" repo >/dev/null; }
update --generate-static-deltas --prune --prune-depth=1
# Deltas leading to a commit that is no longer a ref's head: the prune leaves them behind.
HEADS=$(ostree --repo=repo refs | while read -r r; do ostree --repo=repo rev-parse "$r"; done)
for d in $(ostree --repo=repo static-delta list | grep -E '^[0-9a-f]'); do
    grep -qx "${d##*-}" <<< "$HEADS" || ostree --repo=repo static-delta delete "$d" >/dev/null
done
update

# What a user's flatpak checks: the summary and the commit signed by this key.
ostree init --repo=check --mode=bare-user-only
ostree --repo=check remote add --set=gpg-verify-summary=true --gpg-import="$PUB" here "file://$W/repo" >/dev/null
ostree --repo=check pull --commit-metadata-only here "$APP" >/dev/null || die "signatures not valid for $PUB"
MB=$(( $(du -sb --apparent-size repo | cut -f1) / 1000000 ))
(( MB <= MAX_MB )) || die "repository of $MB MB, over $MAX_MB"
echo "EyeCanSee $VER: commit ${NEW:0:12} (was ${OLD:0:12}), repository $MB MB, signatures valid."

if [[ -n ${OUT_DIR:-} ]]; then
    rm -rf "$OUT_DIR"; mkdir -p "$OUT_DIR"
    tar -C repo -cf "$OUT_DIR.tar" .
    tar -C "$OUT_DIR" -xf "$OUT_DIR.tar"
    echo "written to $OUT_DIR (and $OUT_DIR.tar), nothing uploaded"; exit 0
fi
tar -C repo -cf eyecansee-flatpak-repo.tar .
SHA=$(sha256sum eyecansee-flatpak-repo.tar | cut -d' ' -f1)
gh release create "$TAG" eyecansee-flatpak-repo.tar --repo "$SITE" --draft --title "EyeCanSee $VER" \
    --notes "$(printf 'Flatpak repository of EyeCanSee %s, signed.\n\nSHA-256 of eyecansee-flatpak-repo.tar: `%s`' "$VER" "$SHA")"
echo "Draft $TAG in $SITE. Publish it (gh release edit $TAG --repo $SITE --draft=false) to deploy it to Pages."
