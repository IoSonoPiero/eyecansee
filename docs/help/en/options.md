# Options

The options change how EyeCanSee starts, looks, moves between images, deletes, shows slideshows, plays videos and saves in the editor.

## Opening the options

Choose **Tools > Options…** (![Options…](icon://settings)). The window is divided into tabs: **General**, **Look**, **Browsing**, **Thumbnails**, **Deleting**, **Slideshow**, **Video**, **Editor** and **Resize**. Pointing at many options shows a short explanation.

Every change is saved at once in `settings.json`: in `~/.config/eyecansee` on Linux and in `%APPDATA%\eyecansee` on Windows. It is a text file you can also edit by hand.

![Options](../img/options.webp)

## General

- **Reopen the last folder at startup** (on): when EyeCanSee starts without a folder, for example from the applications menu, it opens the folder you were looking at when you closed it. If that folder no longer exists, or its disk or phone is not connected, it opens your home folder. A saved server opens with its saved password; an FTP server with a user that is not saved opens **File > Open location…** with everything filled in except the password.
- **Window at startup**: **Normal size**, **Maximized** (fills the screen, leaving the system bars visible) or **Size it had when closed** (if it was maximized, it reopens maximized). On Windows a window that is not maximized opens centred in the free area of its screen, above the taskbar, and smaller if it would not fit.
- **Language**: **Same as the system** (English if the system's language is not available) or one of the languages listed, each shown by its own name. The change applies at once, this guide included.
- **Check for updates at startup** (on): see [Updates](updates.md). Not shown in the Flatpak and in the Fedora package, which the system updates; in the copy from the Microsoft Store it asks the Store.

## Look

- **Colors**: **Same as the system** (light or dark as the system, and it follows it when it changes), **Light** or **Dark**. The change applies at once.

## Browsing

- **Remember the scroll position of each folder** (off): going back to a folder you already viewed, the grid starts where you left it. It lasts until EyeCanSee is closed.
- **In the viewer, after the last image start again from the first** (off): and from the first image, going back shows the last one. When off, the viewer stops at the first and the last image.
- **In the viewer, the window's ✕ goes back to the grid** (on): the ✕ at the top right does as Esc. When off, the ✕ closes EyeCanSee from the viewer too.
- **Mouse wheel in the viewer**: **Zooms in and out** (the default) or **Shows the next or previous image**. Holding Ctrl, the wheel always zooms.
- **Largest zoom in the viewer** (2000% at first): how far the wheel and **+** can enlarge an image.

## Thumbnails

- **Clean up the thumbnail cache** (off): once per session, for each location you open, removes the thumbnails of deleted images and makes again those of changed images; then removes the least recently used thumbnails until the cache is within **Cache limit** (1024 MB at first). Folders that cannot be found or reached are skipped, so a disconnected disk or a switched-off NAS keeps its thumbnails.
- **Empty the thumbnail cache…**: deletes all the saved thumbnails, after a confirmation saying how many and how much space is freed. Your images are not touched: the thumbnails of the open folder are made again at once, the others when you open their folder.

## Deleting

- **Delete files on this computer for good, without the trash (dangerous)** (off): deleted files cannot be recovered.
- **Deleting on phones and servers (they have no trash)**: **Move to the hidden .EyeCanSee-trash folder next to the image** (the default) or **Delete for good**.

See [Deleting and the trash](delete.md).

## Slideshow

- **Each image stays for** a number of seconds (5 at first).
- **Passage between images**: **None (straight cut)**, **Crossfade** (the default), **Fade through black**, **Slide**, **Zoom and fade**.
- **Start again after the last image** (on).

See [Slideshow](slideshow.md).

## Video

**Video engine (libmpv) and FFmpeg to use**: libmpv plays the videos, FFmpeg converts them.

- **Automatic: the best for this computer** (the default): the system's when it is recent enough and can play H.264 and HEVC, the formats phones record in, otherwise the downloaded one; for FFmpeg also the downloaded one when it does more of the work on the graphics card.
- **Always the system's, whatever its version**: nothing is downloaded.
- **Always the downloaded one**.

The tab also shows which ones are in use now, with their versions. A change applies the next time EyeCanSee starts. See [Add-on components](components.md).

## Editor

- **Main save button (also Ctrl+S)**: **Save copy** (the default) or **Overwrite the original**. The other one stays in the menu of the arrow next to it; **Save…** in the question on closing follows the same choice. Where the original cannot be overwritten (HEIC, RAW…) the main button is Save copy.
- **After saving**: what happens after **Save copy**, **Update copy** or **Overwrite the original…**. **Stay in the editor** (the default): the editor stays open to go on editing. **Go back to where you were (thumbnails or viewer)**: the editor closes as soon as the file is written; opened from the grid, the grid selects the saved image, opened from the viewer, the viewer shows it. **Go back to the thumbnails**: the editor closes and you are always back in the thumbnails with the saved image selected, also when you opened it from the viewer.

See [Saving an edited image](editor-save.md).

## Resize

The sizes of **Resize**, in the right-click menu of the thumbnails and in **Tools** (see [Converting and resizing images](convert.md)). At first **Small** (long side 1280 px), **Medium** (long side 1920 px), **Large** (long side 2560 px) and **Half** (50%).

Each row is one size: its **Name** (it can stay empty: the menu then shows only the size), the kind (**Percent of the original**, **Inside**, **Long side**, **Short side**, **Width**, **Height**, **Exactly, cropped in the middle**, **Exactly, stretched**) and the pixels or the percent. ![Remove](icon://x) removes it, **Add** adds one, **Back to the first ones** puts back the four sizes you started with. Until you change them, their names follow the language of EyeCanSee.
