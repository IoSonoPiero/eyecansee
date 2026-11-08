# eyecansee-flatpak

The files of the repository IoSonoPiero/eyecansee-flatpak, kept in EyeCanSee's repository until it
exists. It publishes EyeCanSee's own Flatpak repository on GitHub Pages (flatpak.montinaro.info):

- `site/`: the page, `eyecansee.flatpakref`, `eyecansee.flatpakrepo` and the public key, written by
  `make-ref-files.sh <public key> <url of the OSTree repository>`;
- `.github/workflows/pages.yml`: when a release is published, its `eyecansee-flatpak-repo.tar`
  (signed by `tools/sign-flatpak.sh` in EyeCanSee's repository) is checked against the SHA-256 in
  its notes and deployed under `/repo/`, next to `site/`.

The OSTree repository is never committed here: it would grow with every version. Only the latest
release's asset is published.
