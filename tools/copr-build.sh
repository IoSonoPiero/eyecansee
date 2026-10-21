#!/usr/bin/env bash
# Builds the source RPM of the committed HEAD and sends it to COPR (iosonopiero/eyecansee), which
# builds the RPMs for each Fedora version of the project. Needs rpm-build, copr-cli and the API
# token in ~/.config/copr. Usage: tools/copr-build.sh [--srpm-only]
set -euo pipefail
cd "$(dirname "$0")/.."

VER=$(cat VERSION)
OUT=target/srpm
rm -rf "$OUT" && mkdir -p "$OUT/SOURCES" "$OUT/SPECS"
# Committed files only: uncommitted changes and notes/ stay out.
git archive --format=tar.gz --prefix="eyecansee-$VER/" -o "$OUT/SOURCES/eyecansee-$VER.tar.gz" HEAD
sed "s/^Version:.*/Version:        $VER/" eyecansee.spec > "$OUT/SPECS/eyecansee.spec"
rpmbuild -bs --define "_topdir $PWD/$OUT" "$OUT/SPECS/eyecansee.spec"
SRPM=$(ls "$OUT"/SRPMS/eyecansee-"$VER"-*.src.rpm)
echo "$SRPM"
[[ "${1:-}" == "--srpm-only" ]] || copr-cli build eyecansee "$SRPM"
