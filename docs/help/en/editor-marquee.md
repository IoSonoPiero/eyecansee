# Selecting an area

![Selection](icon://square-dashed) **Selection** (M) chooses a part of the image, to copy it, cut it, paste it back as an object, or limit a filter to it.

## Choosing a part

1. Choose ![Selection](icon://square-dashed) **Selection** (M).
2. Drag on the image. Hold Shift for a square.
3. A dashed outline shows the part chosen. Its sides follow the edges of the image's pixels, so a copy keeps the pixels exactly as they are.

A click on the image, or Esc, deselects the part. Esc while a new part is being drawn (with the mouse button still held down) brings back the part chosen before. Choosing another tool also deselects it.

## Copy, cut and paste

The properties bar has three buttons, each with its key:

- ![Copy the part](icon://copy) **Copy the part** (**Ctrl + C**): copies the part as you see it, the photo with what is drawn on it. It also goes to the clipboard, so you can paste it into another program.
- ![Cut the part](icon://scissors) **Cut the part** (**Ctrl + X**): copies the part, then covers it with a solid box. The box takes the colour chosen for the empty corners in [Crop and rotate](editor-crop.md), or white when there is none. As with the [Cover](editor-cover.md) box, nothing of what it hides is left in the saved file.
- ![Paste](icon://clipboard-paste) **Paste** (**Ctrl + V**): puts the part back as a new object, slightly offset from where it was. It is selected, with the Select tool chosen, so you can drag it into place, resize it and rotate it: see below.

## Pasting from another program

**Ctrl + V** also pastes an image copied in another program. It goes in the middle of the result, at most four fifths of the result's size, as an object you can move, resize and rotate. If you copied something in another program after copying a part here, that image is pasted instead. When there is no image to paste, the status bar says so. An image with a side longer than 20,000 pixels, or larger than 200 megapixels in all, is not pasted: a message gives its size.

## Turning, flipping and sizing a pasted piece

A piece you paste, from here or from another program, is a picture you can arrange as you like. Select it (with Select, or right after pasting) and:

- Drag it to move it.
- Drag a corner to resize it. It keeps its proportions; hold **Shift** while dragging to change width and height separately.
- Drag the round handle above it to rotate it by any angle. Hold **Shift** to turn it 15° at a time.

The properties bar has the rest:

- **Rotation**: its angle on the screen, from −180° to 180°.
- ![Rotate 90° anticlockwise](icon://rotate-ccw-square) and ![Rotate 90° clockwise](icon://rotate-cw-square): a quarter turn of the piece alone.
- ![Flip horizontally](icon://square-split-horizontal) and ![Flip vertically](icon://square-split-vertical): mirror the piece left to right or top to bottom, as you see it.
- Width and height in pixels. With **Keep the proportions** on (the default), the other side follows. With it off, each side changes on its own, even when you drag a corner; Shift then keeps the proportions.
- **Opacity**.
- ![Bring to front](icon://to-front) **Bring to front** and ![Send to back](icon://to-back) **Send to back**: over all the other objects, or under them.

Each of these counts as one step of Undo. However many times you turn, flip or resize the piece, it is always drawn again from the pixels you pasted, so it never loses quality, and the saved file is what you see.

## Filters on a part

With a part chosen, the ![Filters](icon://wand-sparkles) **Filters** menu applies the filter to that part only. See [Filters](editor-filters.md).

## Good to know

- Without a part chosen, **Ctrl + C** copies the whole result, and **Ctrl + X** does nothing.
- The part copied is a picture: the objects in it can no longer be changed one by one.
- On Linux with GNOME or X11, other programs receive a copy of at most 16 MB (a PNG image), about 12 megapixels of a photo. A larger copy pastes only in the window it comes from, and the status bar says so.
