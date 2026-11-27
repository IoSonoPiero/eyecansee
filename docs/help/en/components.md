# Add-on components

Some parts of EyeCanSee are large and not everybody needs them, so they are not in the package: EyeCanSee downloads them the first time they are needed, once, after asking you.

## The components

| Component | What it is for | When it is offered |
|---|---|---|
| Video engine (libmpv) | Playing videos and audio files | When the open folder holds a video or an audio file and the computer has no usable libmpv (on Windows, usually the first time) |
| FFmpeg | Converting videos | In the Convert window, when the computer has no usable FFmpeg |
| CAD engine (Open CASCADE) | Opening STEP, IGES and BREP models | When the open folder holds one of those files |

On Linux, EyeCanSee uses the system's libmpv and FFmpeg when they are recent enough (libmpv 1.109, that is mpv 0.33, and FFmpeg 4.3 or later); the download is offered only when they are missing or too old, or when they cannot play H.264 and HEVC, the formats phones record in (Fedora's own packages, for example, lack both). Such an FFmpeg still copies videos and their audio as they are, without the download.

## Downloading one

- **Video engine**: a red **Download the video engine** button appears at the top right of the menu bar, with its size; the viewer offers it too, the first time a video is opened. Until then videos and audio files show their sign (a play button, a note) with a download arrow in the grid. Usually the videos play at once; if the engine cannot start, the button becomes **Restart EyeCanSee**.
- **FFmpeg**: the Convert window shows **Download FFmpeg**, with its size, and a bar while it downloads (see [Converting videos](convert-video.md)).
- **CAD engine**: a red **Download the CAD engine** button appears at the top right of the menu bar, with its size. Until then those models show a cube with a download arrow, and nothing is read from a phone or a server to make their thumbnail. Once it is downloaded, the thumbnails and a model open in the viewer appear by themselves.

Pointing at a red button says what the component is and how large it is.

## At the first start and in the options

The first time EyeCanSee starts, after the page where you choose the language and the colors (on Windows a page then asks whether you want an EyeCanSee icon on the desktop: choose **Yes**, which makes it at once, or **No**, which also takes away the icon **Yes** has just made, and then **Next**; **Next** before an answer stays on the page and says **Choose Yes or No.**), a page lists the components, one card each with what it is for and its size. Nothing is ticked: click the cards of the ones you want and press **Download**, or **Done** to go on without them. If you close the window with cards ticked and not downloaded, also with **Skip** from the first page, EyeCanSee asks first: **Download and close**, **Close without downloading**, or **Back** to the page you were on. **Download and close** downloads them in the window, each with its bar, and closes it by itself once they have all arrived. A download that fails says why in its card, which stays ticked: press **Download** to try again, or **Done** to close. Each download shows its progress in its card and goes on after the window is closed. A component already on this computer has a green mark. The window comes back by itself only when a later version of EyeCanSee brings a new component, and then it shows only that one.

The same list is always in **Tools > Options… > Add-ons**, with **Download** or **Update** on each component (see [Options](options.md)). Each one shows its version next to its name: **Video engine 1.1**, **FFmpeg 9.0**, **CAD engine 1.1** (on Windows the video engine is 1.0).

## Where they go

Each component comes from EyeCanSee's own releases on GitHub. Its SHA-256 fingerprint is written inside the program, so a different file is refused, and nothing is left in place unless the download is complete and checked. Components go into the app's data folder:

- Linux: `~/.local/share/eyecansee/components`
- Flatpak: `~/.var/app/<application id>/data/eyecansee/components`
- Windows: `%LOCALAPPDATA%\eyecansee\components` (the Microsoft Store copy: `components-store`, its own)

## Things to know

- Which libmpv and FFmpeg to use, the system's or the downloaded ones, is chosen in **Tools > Options… > Video** (see [Options](options.md)). The change applies the next time EyeCanSee starts.
- When a new version of EyeCanSee needs a newer component, it asks again: the buttons then say **Update the video engine**, **Update the CAD engine** or **Update FFmpeg**, and the old one is removed once the new one is there.
- On Windows, uninstalling EyeCanSee also removes the downloaded components.
- To remove a component, close EyeCanSee and delete its folder: it is offered again when needed.
