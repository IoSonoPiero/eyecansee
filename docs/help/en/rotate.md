# Rotating and flipping

A photo taken sideways can be turned upright, and a picture can be flipped as in a mirror. In the viewer you see the change first and save it when you want; in the grid the change is saved straight away, also on many pictures at once.

## The commands

They are in the **Image** menu, and in the right-click menu under **Transform**:

- ![Flip horizontally](icon://square-split-horizontal) **Flip horizontally** and ![Flip vertically](icon://square-split-vertical) **Flip vertically**;
- **Rotate 90° clockwise** (**Ctrl + R**), ![Rotate 90° clockwise](icon://rotate-cw-square) in the toolbar;
- **Rotate 90° anticlockwise** (**Ctrl + Shift + R**), ![Rotate 90° anticlockwise](icon://rotate-ccw-square) in the toolbar;
- **Rotate 180°**.

They work on pictures, and the three rotations on videos too (see below); on 3D models and CAD drawings they are greyed out. To turn a picture by any angle, for example to straighten a horizon, use **Image > Rotate and crop…**: see [Crop and rotate](editor-crop.md).

## In the viewer

1. Open the picture and turn or flip it: you see the result at once, and the status bar says **Changed, not saved**.
2. **File > Save** (**Ctrl + S**) saves it over the original, after asking. **File > Save as…** (**Ctrl + Shift + S**) saves a new file in the format you choose, and leaves the original as it is (see [Save as](save-as.md)).

**Ctrl + Z** takes back the last turn or flip not saved yet, **Ctrl + Y** (or **Ctrl + Shift + Z**) does it again. Back as it is on disk, the picture is no longer marked as changed.

If you leave the picture without saving (another picture, back to the grid, a bookmark, an address typed in the bar or opened from the file manager, closing EyeCanSee), EyeCanSee asks first: **Save…**, **Save as…**, **Don't save**, or **Cancel** to stay on the picture. Changing the sort order or renaming the picture does not leave it: the viewer stays on it, with its changes.

## In the grid

Select one or more pictures and choose a turn or a flip from the **Image** menu, the toolbar or the right-click menu, or press **Ctrl + R** or **Ctrl + Shift + R**. A window says how many pictures will be saved over and where the originals go; **Save** does it, **Cancel** (or **Esc**) does nothing. **Enter** does not confirm here, so you cannot overwrite many pictures by mistake. The thumbnails are then made again, and a message titled with what was done (for example **Images rotated**) says how many pictures were saved. When some were skipped, it names them: the first ten, then how many more; ![Copy the message](icon://copy) copies the message with every name.

Pictures in a format EyeCanSee cannot write (HEIC, RAW…) are left out, and the window says how many. Animated pictures, files with several pages and photos with a depth map (the portrait photos of some phones) are skipped too, because saving over them would lose those parts; phone motion photos are turned with their video, and Ultra HDR phone photos with their HDR. When the only picture chosen cannot be saved over itself, the window says why and offers **Save as…**, already turned.

## Videos

A video is turned as a picture is, in the viewer or in the grid, and saved over the original in the same way, with the original going to the trash. In the viewer it turns at once, and **Ctrl + Z** and **Ctrl + Y** take back and redo a turn not saved yet, as for a picture. Once saved, it stays where it was: at the same time, paused or playing as before. It is not encoded again: only the rotation that players read changes, so it takes seconds and loses nothing, and the metadata stay. It works on MP4, MOV, MKV and WebM, with an FFmpeg (see [Converting videos](convert-video.md)): if EyeCanSee has none to use, it offers to download it when you save. A video cannot be flipped this way: to flip it, use **Convert…**. On other videos (AVI, MPEG…) the rotations and the flips are greyed out, in the right-click menu the whole of **Transform**: flip and turn them with **Convert…**. **Rotate and crop…** and **Edit image…** work on pictures only: on a video they are greyed out too, and pointing at them says why.

## Saving over the original

- The original goes where a deletion would send it, usually the trash, so you can get it back. See [Deleting and the trash](delete.md).
- A JPEG is turned without losing quality when its width and height are multiples of 16 pixels; otherwise it is saved again at the quality it had. WebP, AVIF and GIF are saved again too, and may lose a little quality. The confirmation says so.
- The metadata (date, camera, place) stays in the file.
- Phone photos with Ultra HDR keep their HDR, and motion photos their video, shown turned as the photo (the video is not encoded again).
- A picture that would need more than half of the computer's memory to be saved again (for example a HEIC, a 16-bit TIFF or a PNG of hundreds of megapixels, on a computer with 8 GB) is not saved at once: a window says how much memory it needs and offers **Open anyway** (the computer may then become very slow, or the system may close EyeCanSee); the choice holds for that file until EyeCanSee is closed. On the grid such a picture is left out, and the message at the end names it. A JPEG turned without losing quality is never asked about: it is not decoded.
- On a folder where saving over a file is not safe (for example a phone opened through the file manager), Save explains why and offers **Save as…**.
