# eyecansee-flatpak

The Flatpak repository of [EyeCanSee](https://github.com/IoSonoPiero/eyecansee), an image and video
viewer, manager and editor, signed by the developer. It is published at
https://flatpak.montinaro.info. EyeCanSee is not on Flathub.

Signing key fingerprint: `11F7D1A7D8D356D2FDE8AC47D54E92F7B64C427D`
(the public key is in [site/eyecansee-flatpak.gpg](site/eyecansee-flatpak.gpg) and inside the
`.flatpakref` and `.flatpakrepo` files).

## Install

With one click: open https://flatpak.montinaro.info/eyecansee.flatpakref with your software center.

From a terminal:

```
flatpak install https://flatpak.montinaro.info/eyecansee.flatpakref
```

Updates come with `flatpak update` or your software center.

## What is here

The releases of this repository only publish the signed Flatpak repository: each one carries
`eyecansee-flatpak-repo.tar`, signed on the developer's PC (`tools/sign-flatpak.sh` in EyeCanSee's
repository), and its publication deploys it to GitHub Pages with
[.github/workflows/pages.yml](.github/workflows/pages.yml). The page, the `.flatpakref` and the
`.flatpakrepo` are in [site/](site/). The Flatpak repository itself is never committed here.
