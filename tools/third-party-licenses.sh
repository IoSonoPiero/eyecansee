#!/usr/bin/env bash
# Linux version of third-party-licenses.ps1: same THIRD-PARTY-LICENSES.txt, from cargo metadata and jq.
# Standard licence texts from tools/licenses/ (SPDX License List): no network, as Flathub builds.
# Usage: tools/third-party-licenses.sh <output file> [cargo feature flags...]
#   e.g. tools/third-party-licenses.sh THIRD-PARTY-LICENSES.txt --no-default-features --features heif
set -euo pipefail
OUT=$(realpath -m "$1"); shift
cd "$(dirname "$0")/.."

json=$(cargo metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu "$@" 2>/dev/null)
# name, version, licence, repository, authors, folder of every linked crate except EyeCanSee itself,
# separated by \x1f: a tab would collapse empty fields in `read`
pkgs=$(jq -r '([.resolve.nodes[].id]) as $ids | [.packages[] | select(.id as $i | $ids | index($i))
    | select(.name != "eyecansee")] | sort_by(.name, .version)[]
    | [.name, .version, (.license // ""), (.repository // ""), (.authors | join(", ")),
       (.manifest_path | sub("/Cargo.toml$"; ""))] | join("\u001f")' <<<"$json")

missing=()
# A crate that builds C or C++ sources of its own (a build script, and such sources outside its
# tests) puts a library inside the executable whose copyright the licence asks to repeat. Its
# licence file is in a subfolder (libjpeg-turbo, libheif, libwebp, ring); where the package has
# none, our copy of it is in tools/licenses/c/<crate>.txt (meshoptimizer, ufbx); or the crate is
# here, with the reason no notice is needed. Any other one stops the script (audit 7.7).
declare -A C_NO_NOTICE=(
    [libsqlite3-sys]="SQLite is in the public domain"
    [wayland-backend]="a small shim of the crate itself, under the crate's licence"
)
unlisted=()
{
    echo "Third-party software included in EyeCanSee"
    echo "=========================================="
    echo
    echo "EyeCanSee is built with the following libraries. Their licences and copyright notices follow."
    echo
    while IFS=$'\x1f' read -r name ver lic repo authors dir; do echo "- $name $ver ($lic)"; done <<<"$pkgs"
    while IFS=$'\x1f' read -r name ver lic repo authors dir; do
        # Bundled C libraries keep their licence in a subfolder (libjpeg-turbo, SQLite, libheif).
        mapfile -t files < <( (find "$dir" -maxdepth 1 -type f -regextype egrep \
                -regex '.*/(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|COPYRIGHT)[^/]*'
            find "$dir" -mindepth 2 -maxdepth 3 -type f -regextype egrep \
                -regex '.*/(LICEN[CS]E|COPYING|README\.ijg)[^/]*' -not -regex '.*/(tests?|examples|target)/.*') | sort)
        echo
        printf '=%.0s' {1..78}; echo
        echo "$name $ver  -  $lic"
        [[ -n $repo ]] && echo "$repo"
        printf '=%.0s' {1..78}; echo
        [[ -n $authors ]] && echo "Copyright: $authors"
        inside=$(find "$dir" -mindepth 2 -maxdepth 3 -type f -regextype egrep \
            -regex '.*/(LICEN[CS]E|COPYING|README\.ijg)[^/]*' -not -regex '.*/(tests?|examples|target)/.*' -print -quit)
        csrc=
        if [[ -f $dir/build.rs ]]; then
            csrc=$(find "$dir" \( -path '*/test/*' -o -path '*/tests/*' -o -path '*/examples/*' -o -path '*/benches/*' \
                -o -path '*/target/*' \) -prune -o -type f \( -name '*.c' -o -name '*.cc' -o -name '*.cpp' -o -name '*.cxx' \) -print -quit)
        fi
        if [[ -n $csrc && -z $inside && -z ${C_NO_NOTICE[$name]:-} ]]; then
            unlisted+=("$name $ver")
        fi
        if [[ ${#files[@]} -eq 0 ]]; then
            missing+=("$name $ver ($lic)")
            echo "(no licence file in the package: the full text of $lic is at the end of this file)"
            continue
        fi
        for f in "${files[@]}"; do echo; echo "--- ${f#"$dir"/}"; cat "$f"; echo; done
    done <<<"$pkgs"
    # Not a crate: the icons of the buttons, inside the executable.
    echo
    printf '=%.0s' {1..78}; echo
    echo "Lucide icons (some derived from Feather), inside eyecansee  -  ISC AND MIT"
    echo "https://lucide.dev"
    printf '=%.0s' {1..78}; echo
    echo; cat assets/icons/lucide/LICENSE-lucide.txt; echo
    echo
    printf '=%.0s' {1..78}; echo
    echo "Standard licence texts (from the SPDX License List)"
    printf '=%.0s' {1..78}; echo
    for id in MIT Apache-2.0 BSD-3-Clause BSL-1.0 OFL-1.1 Ubuntu-font-1.0; do
        echo; echo "--- $id"
        cat "tools/licenses/$id.txt"; echo
    done
} > "$OUT"
echo "crates: $(wc -l <<<"$pkgs")"
echo "without licence file: ${#missing[@]}"
printf '%s\n' "${missing[@]}"
if [[ ${#unlisted[@]} -gt 0 ]]; then
    echo "C or C++ inside without its copyright notice (add tools/licenses/c/<crate>.txt):" >&2
    printf '%s\n' "${unlisted[@]}" >&2
    exit 1
fi
