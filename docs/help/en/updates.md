# Updates

EyeCanSee can keep itself up to date. How depends on how you installed it.

## Which copies update themselves

- **Windows installer, Windows zip and AppImage**: EyeCanSee updates itself, as described below.
- **Microsoft Store**: the Store updates it. EyeCanSee asks the Store whether it has a newer version: if so, a red **Update to … in the Microsoft Store** button appears at the top right of the menu bar, and clicking it opens EyeCanSee's page in the Store, where you update it.
- **Flatpak**: it is updated by `flatpak update` or by your software center (for example GNOME Software, Discover or the Linux Mint Update Manager).
- **Fedora package**: it is updated with the rest of the system, by `sudo dnf upgrade` or by your software center (GNOME Software, Discover). On Fedora Atomic (Silverblue, Kinoite), where it is layered with `rpm-ostree`, by `rpm-ostree upgrade` and a restart, or by the software center. It has no **Check for updates at startup** option.

## How it works

1. At startup EyeCanSee asks GitHub, in the background, for the latest published version.
2. If it is newer, EyeCanSee downloads it and checks it twice: against the SHA-256 fingerprint GitHub publishes for it (a broken download), and against the developer's signature, made with a key that never leaves the developer's computer (a file that does not come from the developer). A file without a valid signature is never installed.
3. When it is ready, a red **Install … and restart** button appears at the top right of the menu bar, with the new version number.
4. Click it: EyeCanSee closes, installs the new version and opens again. Nothing is installed until you click it.

## Checking by hand

**Help > Check for updates** (![Check for updates](icon://download)) does the same check at any time and tells you the outcome: that you have the latest version, that a new one is ready (then click the red button), or why this copy is not updated by EyeCanSee (Flatpak, Fedora package, or a development build). In the copy from the Microsoft Store it asks the Store. If the check fails, it says why.

## Turning the check off

Untick **Check for updates at startup** in **Tools > Options… > General**. EyeCanSee then contacts GitHub (or, in the copy from the Microsoft Store, the Store) only when you choose **Check for updates**. Nothing about you or your images is ever sent: only the request for the latest version.
