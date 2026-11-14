# Converting and resizing images

Converts many pictures at once: into another format, smaller or larger, turned or flipped, into the same folder, a subfolder or another folder, also on another device. The originals are never touched: the new files are written beside them or where you choose.

For one image with a live preview use [Saving in another format](save-as.md). Videos go through the same window: see [Converting videos](convert-video.md).

## Starting

1. In the grid, select the pictures (see [Selecting files](selection.md)). With nothing selected, the image under the cursor is used.
2. Click ![Convert…](icon://scaling) in the toolbar, or choose **Tools > Convert…** (**Ctrl+B**), or right-click one of the selected files and choose **Convert…**. The window opens on its format, with the size **Unchanged**; **Resize > Custom…** opens it on its size (see below).
3. Make your choices in the window and click **Convert**.

The window works on pictures and videos. Other files you selected (3D models, CAD drawings, audio files) are left out: the window says how many, the message at the end which ones. It does not work from the viewer, and only one conversion runs at a time.

![Converting and resizing images](../img/convert.webp)

## Resizing in one click

To only make smaller copies of photos, without the window: right-click the selected files, or open **Tools**, and choose **Resize** and one of its sizes: **Small (long side 1280 px)**, **Medium (long side 1920 px)**, **Large (long side 2560 px)** or **Half (50%)**. The sizes are yours to change in [Options](options.md), on the **Resize** tab.

The work starts at once on every picture chosen, with the same box and **Cancel** as below and the message at the end. The copies go next to the originals, in their format and with their metadata (an **Ultra HDR** photo stays Ultra HDR), the size in their names: `photo (1920 px).jpg`, `photo (50%).jpg`. A name already taken gets " (2)". Pictures already of that size, or smaller (they are never enlarged), get no copy, and the message names them: so resizing a copy again does not make `photo (1920 px) (1920 px).jpg`. Pictures in a format EyeCanSee cannot write (HEIC, RAW…) are left out, and the message says why. Videos, 3D models, CAD drawings and audio files are left out too, and the message names them: with some of them among the files chosen, **Resize** is marked with **\***; with no picture at all, it is greyed out.

**Resize > Custom…** opens this window, called **Resize** and with its button **Resize**, on the pictures, on the turn and the size, with the format **Same as the original** and the copies next to the originals. It starts from the size of the last Custom… (at first **Long side** 1920 px); **Convert…** instead always starts from **Unchanged**. As with the sizes above, the copies have the size in their names (with **Unchanged**, what was done), in whatever format and wherever they go:

| Size | Name of the copy |
|---|---|
| Long side, Short side, Width or Height of 600 px | `photo (600 px).jpg` |
| Inside 800 × 600 | `photo (800x600).jpg` |
| Percent of the original, 50 | `photo (50%).jpg` |
| Exactly 800 × 600, cropped or stretched | `photo (800x600).jpg` |
| Unchanged, turned (flipped too or not) | `photo (rotated).jpg` |
| Unchanged, only flipped | `photo (flipped).jpg` |

With a size and a turn, the name says only the size. A name already taken gets " (2)" after it: `photo (600 px) (2).jpg`, `photo (rotated) (2).jpg`.

## Format

- **Same as the original**: each image stays in its own format and is written as Save writes it (a JPEG keeps its own quality). Images in a format EyeCanSee cannot write (HEIC, RAW…) are left out.
- One of the formats of [Saving in another format](save-as.md). Its options are under **JPEG options** (or the name of the format you chose): click the title to open them. **Keep the metadata** is there too.
- **Ultra HDR** phone photos stay HDR only in JPEG. When JPEG photos are among the files and you choose another format, the window warns you.

## Turn and flip

- **No turn**, **90° clockwise**, **180°** or **90° anticlockwise**.
- **Flip horizontally** and **Flip vertically**. With a turn too, the flips come first, then the turn.
- JPEG photos that are only turned, in their own format, are turned without losing quality when their size allows it, as Save does.

## Size

- **Unchanged**.
- **Percent of the original** (50 % at first).
- **Inside** a box of pixels (1920 × 1080 at first): each image is made as large as fits in the box, keeping its proportions, after the turn.
- **Long side**, **Short side**, **Width** or **Height** of so many pixels, keeping the proportions: a long side of 1920 makes landscape and portrait photos alike 1920 pixels on their longer side.
- **Exactly, cropped in the middle**: exactly the size written; what does not fit the proportions is cut off equally on both sides.
- **Exactly, stretched**: exactly the size written, the picture stretched or squeezed to it.
- With videos among the files chosen, only **Unchanged**, **Percent of the original** and **Inside** can be chosen: the other sizes are for pictures only.
- **Enlarge smaller files too**: off at first, so images already smaller than the size asked stay as they are (with an exact size, no side grows larger than the original's). In the format **Same as the original**, a picture that would come out the same as the original gets no copy, and the message names it.
- **Filter**, how the new pixels are worked out: **Lanczos** (sharpest, the default), **Catmull-Rom**, **Gaussian** (soft), **Bilinear** (fast), **Nearest pixel** (keeps the hard edges of pixel art).

If nothing would change (same format, same size, no turn), **Convert** (or **Resize**) stays off and a red line says so: the images would only lose quality.

## Where

- **Next to the originals**.
- **In the subfolder**, with the name you write ("converted" at first). It is created if missing.
- **In another folder, also on another device**: type a folder or an address as in the address bar (`mtp://…` for a phone, `sftp://…` for a server), or click **Browse…**. The chooser shows your home folder, the disks, the connected phones and the saved servers on the left: click a folder to go into it, then **Use this folder**. The folder must already exist.

A name already taken gets " (2)": nothing is ever written over a file. So you can, for example, convert the photos of a phone straight into a folder of the computer.

The window remembers your choices until EyeCanSee is closed.

## While it works

A box at the bottom right shows how many files are done, a bar and the file under way, with **Cancel**. You can go on using EyeCanSee meanwhile. Cancel stops after the image under way.

At the end a message says how many files were saved and where, and which were left out and why.

## Things to know

- Animated images and images with several pages are left out: the new file would keep only the first picture.
- A phone's motion photo keeps its video only when it is just turned and stays a JPEG; otherwise the new file is the picture alone, and the video stays in the original.
- It works on every source: the computer, disks, phones and servers.
- In the window **Enter** starts the conversion (when no field is being typed in) and **Esc** cancels.
