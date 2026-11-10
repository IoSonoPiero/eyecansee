# Saving an edited image

The editor never changes the original unless you ask. The main button, **Save copy**, writes a new file next to it.

## Save copy

**Save copy** (Ctrl+S) makes a copy next to the original:

- Its name is the original's with "(edited)", for example `beach (edited).jpg`; "(edited 2)" when that name is taken.
- Its format is the original's when EyeCanSee can write that format (JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF, QOI). Otherwise it is a PNG when the image has transparency or comes from an OpenEXR or Radiance HDR file, and a JPEG in the other cases (a HEIC, a RAW or a PSD, for example).
- Before writing, a window gives the copy's name and what it loses, if anything: encoded again with a little loss, 8 bits per colour instead of more, what is brighter than white. **Save** writes it; **Cancel** goes back.

The original stays as it is. The page stays open, so you can go on. When you close it, the grid selects the copy and the viewer shows it.

If a save fails, a yellow band with a warning triangle at the bottom of the window says why in plain words (for example that the folder is read-only or the disk is full), and the page stays open with your changes.

## Update copy

After the first copy is written the button becomes **Update copy**. It writes the same copy again, always from the original with all the changes, so nothing is compressed twice. If the copy was deleted or another program changed it, a new copy is made and the status bar says so.

A save that fails changes nothing here: the button stays as it was, and your changes are still there to save. If no copy was written before, the next save makes a new one; if one was, it updates that one.

The button is off when there is nothing new to save.

## More ways to save

The arrow ![More ways to save](icon://chevron-down) next to Save copy has two more:

- ![Save as…](icon://file-output) **Save as…** (Ctrl+Shift+S): opens [Saving in another format](save-as.md) with the result, to choose the name, the format and the folder.
- ![Overwrite the original…](icon://save) **Overwrite the original…**: writes the edited image over the original, after asking. The original goes to the trash, even when deleting for good is chosen in the [Options](options.md). If it cannot go to a trash, nothing is overwritten. The page stays open; a second Overwrite starts again from the original as it was opened and replaces what was written before, so only the true original is in the trash. See [Deleting and the trash](delete.md).

Overwrite is off, and pointing at it says why, for a new image, for a format EyeCanSee does not write back (HEIC, RAW…), and in places where EyeCanSee never writes over a file (a folder opened through the file manager's gvfs link, a temporary copy made by Windows).

## What the saved file keeps

- A JPEG saved as JPEG keeps the quality it was saved with, so it is not compressed harder. A picture of another format saved as JPEG is written at quality 95.
- The metadata (EXIF, XMP, IPTC, colour profile) stay, with the new size, without the small previews of the original.
- An Ultra HDR photo from a phone stays HDR. The video of a motion photo is not kept, unless you only turned or flipped it.
- Every object, filter and crop is applied at full size.

## Closing with changes not saved

Close, the window's ✕ or Esc first ask what to do:

- **Save…**: a copy, as Save copy does (Save as… for a new image). Once you confirm, the page closes as soon as the copy is written. If saving fails, the page stays open with your changes.
- **Save as…**: closes the page and opens Saving in another format.
- **Don't save**: closes and forgets the changes.
- **Cancel**: back to the page.

## Copy to the clipboard

![Copy](icon://copy) **Copy** (Ctrl+C) puts the whole result, at full size, on the clipboard, to paste it into another program. Nothing is written to disk.

## Phones and servers

An image on a phone or a server is edited in a copy on this computer, and the status bar says so. The new file is kept on the computer first, then sent. If sending fails, it stays on the computer, and a window offers **Try again**, **Save as…** and **Open the folder**. See [Android phones](phone.md) and [Servers (FTP and SFTP)](servers.md).

A phone photo opened from File Explorer on Windows is a temporary copy made by Windows: Save copy goes straight to Save as… there.
