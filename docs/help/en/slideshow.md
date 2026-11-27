# Slideshow

The slideshow shows the pictures one after the other at full screen, on black, with a transition between them.

## Starting the slideshow

- **View > Slideshow**, or ![Slideshow](icon://presentation) **Slideshow** in the right-click menu of a picture, in the grid or in the viewer.
- With several files selected, it shows only the pictures and videos among them, always starting from the first one in the grid.
- Otherwise it shows the pictures and videos of the folder, starting from the one chosen (or the one open in the viewer). If the file chosen is not a picture or a video, it starts from the first picture of the folder.
- 3D models, CAD drawings and audio files are never shown. In a folder without pictures or videos, **View > Slideshow** is greyed out.

## During the slideshow

| Key | What it does |
|---|---|
| Esc, double click | Ends the slideshow |
| Space | Pauses and goes on |
| →, Page Down | Next picture at once |
| ←, Page Up | Previous picture at once |

The keys are shown at the bottom for the first seconds; while paused, the screen says **Paused (Space to resume)**.

A video plays to the end, with its audio, then the next picture comes; **Space** pauses it too. See [Playing videos](video.md).

When the slideshow ends you are back where you were, on the last picture shown.

## Settings

In **Tools > Options… > Slideshow**:

- **Each image stays for**: how many seconds (5 by default).
- **Transition between images**: **None (straight cut)**, **Crossfade** (the default), **Fade through black**, **Slide** or **Zoom and fade**. With **Slide**, when you go back, the pictures slide the other way.
- **Start again after the last image** (on by default): when off, the slideshow ends after the last picture.

## Good to know

The next picture is read in advance. If it is not ready yet (a large file, a phone, a server), the current one stays until it is.

A picture or a video that cannot be opened (a damaged file) is skipped: for a few seconds a small notice with a warning triangle in the top left corner names it, without covering the pictures that follow. A video on a phone or a server that gives no data for 20 seconds is skipped the same way, and the slideshow goes on. If none of the files can be opened, the slideshow stops and a message at the bottom of the window says so; when the grid already shows every one of them with the warning triangle, it does not start at all, and only the message comes.
