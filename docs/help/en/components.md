# Add-on components

Some parts of EyeCanSee are large and not everybody needs them, so they are not in the package: EyeCanSee downloads them the first time they are needed, once, after asking you.

## The components

| Component | What it is for | When it is offered |
|---|---|---|
| Video engine (libmpv) | Playing videos and audio files | When the open folder holds a video or an audio file and the computer has no usable libmpv (on Windows, usually the first time) |
| FFmpeg | Converting videos | In the Convert and resize window, when the computer has no usable FFmpeg |
| CAD engine (Open CASCADE) | Opening STEP, IGES and BREP models | When the open folder holds one of those files |

On Linux, EyeCanSee uses the system's libmpv and FFmpeg when they are recent enough (libmpv 1.109, that is mpv 0.33, and FFmpeg 4.3 or later); the download is offered only when they are missing or too old.

## Downloading one

- **Video engine**: a red **Download the video engine** button appears at the top right of the menu bar, with its size; the viewer offers it too, the first time a video is opened. Usually the videos play at once; if the engine cannot start, the button becomes **Restart EyeCanSee**.
- **FFmpeg**: the Convert and resize window shows **Download FFmpeg**, with its size, and a bar while it downloads (see [Converting videos](convert-video.md)).
- **CAD engine**: a red **Download the CAD engine** button appears at the top right of the menu bar, with its size. Until then those models show a cube with a download arrow, and nothing is read from a phone or a server to make their thumbnail.

Pointing at a red button says what the component is and how large it is.

## Where they go

Each component comes from EyeCanSee's own releases on GitHub. Its SHA-256 fingerprint is written inside the program, so a different file is refused, and nothing is left in place unless the download is complete and checked. Components go into the app's data folder:

- Linux: `~/.local/share/eyecansee/components`
- Flatpak: `~/.var/app/<application id>/data/eyecansee/components`
- Windows: `%LOCALAPPDATA%\eyecansee\components`

## Things to know

- Which libmpv and FFmpeg to use, the system's or the downloaded ones, is chosen in **Tools > Options… > Video** (see [Options](options.md)). The change applies the next time EyeCanSee starts.
- When a new version of EyeCanSee needs a newer component, it asks again, and the old one is removed once the new one is there.
- On Windows, uninstalling EyeCanSee also removes the downloaded components.
- There is no window to manage the components yet. To remove one, close EyeCanSee and delete its folder: it is offered again when needed.
