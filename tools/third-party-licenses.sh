#!/usr/bin/env bash
# Linux version of third-party-licenses.ps1: same THIRD-PARTY-LICENSES.txt, from cargo metadata and jq.
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
        if [[ ${#files[@]} -eq 0 ]]; then
            missing+=("$name $ver ($lic)")
            echo "(no licence file in the package: the full text of $lic is at the end of this file)"
            continue
        fi
        for f in "${files[@]}"; do echo; echo "--- ${f#"$dir"/}"; cat "$f"; echo; done
    done <<<"$pkgs"
    echo
    printf '=%.0s' {1..78}; echo
    echo "Standard licence texts (from the SPDX License List)"
    printf '=%.0s' {1..78}; echo
    for id in MIT Apache-2.0 BSD-3-Clause BSL-1.0 OFL-1.1 Ubuntu-font-1.0; do
        echo; echo "--- $id"
        curl -sSf "https://raw.githubusercontent.com/spdx/license-list-data/main/text/$id.txt"; echo
    done
} > "$OUT"
echo "crates: $(wc -l <<<"$pkgs")"
echo "without licence file: ${#missing[@]}"
printf '%s\n' "${missing[@]}"
