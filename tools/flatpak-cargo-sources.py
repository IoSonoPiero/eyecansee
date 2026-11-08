#!/usr/bin/env python3
"""Cargo.lock -> flatpak/cargo-sources.json: every crate from crates.io as a flatpak-builder
source, so the Flatpak builds offline (`cargo build --offline`). Run after every change of
Cargo.lock:  tools/flatpak-cargo-sources.py > flatpak/cargo-sources.json
ponytail: crates.io only (Cargo.lock has no git sources); flatpak-cargo-generator if one comes."""
import json, sys, tomllib

lock = tomllib.load(open(sys.argv[1] if len(sys.argv) > 1 else "Cargo.lock", "rb"))
out = []
for p in lock["package"]:
    src = p.get("source", "")
    if not src:
        continue  # the app itself and the patches in vendor/
    if src != "registry+https://github.com/rust-lang/crates.io-index":
        sys.exit(f"unsupported source for {p['name']}: {src}")
    name, ver, sha = p["name"], p["version"], p["checksum"]
    dest = f"cargo/vendor/{name}-{ver}"
    out.append({"type": "archive", "archive-type": "tar-gzip", "url": f"https://static.crates.io/crates/{name}/{name}-{ver}.crate", "sha256": sha, "dest": dest})
    out.append({"type": "inline", "contents": json.dumps({"package": sha, "files": {}}), "dest": dest, "dest-filename": ".cargo-checksum.json"})
out.append({"type": "inline", "contents": '[source.vendored-sources]\ndirectory = "cargo/vendor"\n\n[source.crates-io]\nreplace-with = "vendored-sources"\n', "dest": "cargo", "dest-filename": "config"})
json.dump(out, sys.stdout, indent=4)
print()
