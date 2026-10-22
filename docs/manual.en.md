# EyeCanSee manual

Press **F1** at any time to open or close this manual.

## The window

- **Menu bar**: File, View, Go, Help. The version number is on the right; **Help → About EyeCanSee** shows the version, the author, the license and the link to the source code. Every action you can do with the keyboard or a double click also has a menu item, with its key shown next to it.
- **Toolbar**: parent folder, refresh, metadata (ℹ), the address bar, the image order and the thumbnail size slider.
- **Folder tree** on the left, under **This computer** (and under **Devices** if you have any connected): click a folder to show its content. Folders open only when you expand them, so remote sources stay fast. When you change folder, also from the grid, the tree opens down to that folder and highlights it.
- **Grid** on the right: the **..** cell to go up one level (not at the top of a disk or device), the subfolders, then the images as thumbnails, spread over the whole width. When you enter a folder nothing is selected; when you go up, the folder you came from is selected.
- **Image order** from the drop-down list in the toolbar (with the ▲/▼ button next to it to reverse it) or from **View → Sort by**: by name, modification date, size or type (extension), plus **Descending order**. Folders stay first, by name; files without a date stay at the end. The choice is remembered.
- **Metadata** below the grid, for the selected image: boxes **Camera** (make, model, lens, software), **Shot** (date, exposure, aperture, ISO, focal length, flash), **Image** (size, color space) and **Location** (GPS coordinates, altitude, with a link to open them on the OpenStreetMap map). Each box closes with its X and reopens from **View → Metadata boxes**; the ℹ button in the toolbar closes or reopens them all. The panel collapses and reopens with the ▼/▲ button at the right of its bar, or with a click on the bar itself; when collapsed only the bar stays, and the metadata is not read. EXIF metadata is read from JPEG, HEIC, AVIF, PNG, WebP and TIFF; to read it EyeCanSee reads the file again, so on a phone it takes a moment.
- **Status bar** at the bottom: how many folders and images there are, and name, size and modification date of the selected item; with several items selected, how many and the size of their images.
- **Several items**: Ctrl+click adds or removes an item, Shift+click selects everything from the last clicked item to this one, Shift+arrows extend the selection, Ctrl+A selects everything in the folder. **Dragging** with the mouse draws a rectangle that selects the cells it touches (with Ctrl it adds them to those already selected); near the top or bottom edge the grid scrolls. A click without keys, or an arrow, goes back to a single item.
- Arrows and Tab act on the side you clicked last, left or right: it is the one with the thin frame. There the selection is solid blue, on the other side it is lighter.

Double-click an image, or select it and press Enter, to open it in the **viewer**. Press Esc, double-click the image or use the grid button in the toolbar to go back to the grid.

## Thumbnails

EyeCanSee creates the thumbnails in this order: the selected image first, then the ones visible in the window from top to bottom, then all the others in the folder from top to bottom. Clicking an image moves its thumbnail to the front of the queue. Up to 1000 thumbnails per folder stay in memory, so they do not disappear while scrolling, even when dragging the scrollbar; in bigger folders the ones far from the visible part are reloaded from disk.

Thumbnails are saved on disk, so opening the same folder again is immediate: in the file `~/.cache/eyecansee/thumbs.db` on Linux and `%LOCALAPPDATA%\eyecansee\thumbs.db` on Windows. If an image changes (size or modification date), its thumbnail is created again. You can delete that file (with EyeCanSee closed) at any time: thumbnails are created again when needed.

**Clean up the thumbnail cache** in **File > Options…** (off by default): once per session, for each opened location, removes thumbnails of deleted images and recreates those of changed images; then removes the least recently used thumbnails until the cache is within **Cache limit** (default 1024 MB). Folders that cannot be found or reached are skipped, so a disconnected disk or a switched-off NAS does not lose its thumbnails. Remote locations are checked only when you open them.

## Options

**File > Options…** opens the options window, divided into tabs: **General** (last folder, window at startup, language), **Look** (colors), **Browsing** (scrolling, viewer), **Deleting** and **Thumbnails**. Every change is saved immediately in `~/.config/eyecansee/settings.json` on Linux and `%APPDATA%\eyecansee\settings.json` on Windows, a text file you can also edit by hand.

- **Reopen the last folder at startup** (on by default): when EyeCanSee starts without a folder, for example from the applications menu, it opens the folder you were viewing when you closed it. If that folder no longer exists, it opens your home folder. Remote locations reopen without the password, which you must type again.
- **Remember the scroll position of each folder** (off by default): when you go back to a folder you already viewed, the grid starts where you left it. When off, every folder you open starts from the first thumbnail. It lasts until EyeCanSee is closed.
- **In the viewer, after the last image start again from the first** (off by default): and from the first image, going back shows the last one. When off, the viewer stops at the first and the last image.
- **Window at startup**, one choice only: **Normal size**, **Maximized** (fills the screen, leaving the system bars visible), or **Size it had when closed** (if it was maximized, it reopens maximized).
- **Language**: **Same as the system** (default: the Windows or desktop language, English if it is not among the available ones) or one of the listed languages, each shown by its own name. The choice applies at once, this manual included.
- **Colors**: **Same as the system** (default: light or dark like the Windows or desktop theme, following it when it changes), **Light** or **Dark**. The choice applies at once.
- **Clean up the thumbnail cache** and **Cache limit**: see *Thumbnails*.
- **Empty the thumbnail cache…**: deletes all saved thumbnails, after a confirmation showing how many and how much space is freed. Your images are not touched; thumbnails of the open folder are created again right away, the others when you open them again.
- **Delete files on this computer for good, without the trash (dangerous)** (off at first): see *Deleting and renaming*.
- **Deleting on phones and servers**: see *Deleting and renaming*.

## Deleting and renaming

**Del** (or **File > Delete…**) deletes the image or folder selected in the grid, or the image open in the viewer, after a confirmation showing its name and size. A folder is deleted with everything inside it. Folders in the tree on the left are deleted with a right click. In the confirmation **Enter** deletes and **Esc** cancels. In the viewer the next image appears at once.

With several items selected, **Del** (or **Delete…** from the right click on one of them) deletes all of them, after a confirmation with how many they are and the size of the images. If one fails, the following ones are not deleted. While a deletion runs the status bar says so, with a spinner, and no other deletion can be started (on a phone a folder with hundreds of files takes several seconds).

- **Files on this computer and external disks** go to the system trash, where they can be restored. With the option **Delete files on this computer for good, without the trash** they are deleted forever.
- **Phones and servers (FTP, SFTP)** have no trash EyeCanSee can reach. In the Options, under **Deleting on phones and servers**, choose between **Move to the hidden .EyeCanSee-trash folder next to the image** (default: the image goes into a `.EyeCanSee-trash` folder inside its own folder, and you get it back with a file manager) and **Delete for good**. FTP cannot move files: there you need **Delete for good**.

**Shift+Del** (or **File > Delete for good…**) deletes forever that one time, whatever the option says.

**F2** (or **File > Rename…**) renames the selected item or the open image: type the new name and press **Enter** (**Esc** cancels). EyeCanSee does not accept a name already in the folder. On SFTP servers folders cannot be renamed, and over FTP nothing can.

The top folder of a source and the storages of a phone cannot be deleted or renamed.

**The trash of phones and servers**: every folder where you deleted something has a **Trash**: a strip at the top of the grid says so, with the **Open the trash** button. Inside it the strip says whose trash it is and has **Restore all** and **Empty the trash…**. Inside it you can look at the images, **restore** them (they go back to the folder containing the trash) or delete them for good: there **Del** always deletes forever, and there is no Rename. Right click on an empty space inside the Trash: **Restore all** and **Empty the trash…** (everything deleted forever, after a confirmation). A trash left empty, after restoring or deleting what was inside it, goes away together with its strip.

If the name of an item being restored is already taken, EyeCanSee asks what to do: for a file **Replace** (the one already there is deleted) or **Keep both** (the restored one becomes for example `photo (2).jpg`); for a folder **Merge** (the contents add up; files with the same name are both kept) or **Keep both**. **Cancel** also stops the rest of a Restore all. In the trash too a name already there becomes `photo (2).jpg`.

## Copying and moving

**Ctrl+C** copies the selected items (or the image open in the viewer), **Ctrl+X** cuts them; then open the folder where they should go and press **Ctrl+V**. Each copy or cut is pasted once: to paste again, copy again. The same items are in the **File** menu and in the right click; the right click on a folder also has **Paste into this folder**. Files and folders, with everything inside them, go anywhere: another folder of the same disk, another disk, a phone, a server, also from one device to another.

- Copied items get a blue badge in the corner of their cell, cut items an orange one with scissors and fade; the status bar says how many are waiting for **Ctrl+V**.
- A window at the bottom right shows how many files and how much data have been copied, with **Cancel**: the file being copied when you cancel is not left halfway, the ones already copied stay. Towards a phone, Cancel waits for the end of the file being copied (a phone receiving half a file stops answering). Closing the window during a copy cancels it, and the window closes once the file being copied is finished.
- If an item with the same name is already there, EyeCanSee asks: **Replace** (for a folder **Merge**: its contents go into the existing folder, replacing the files with the same name), **Skip** or **Keep both** (the new one becomes "name (2)"). With several such items, a check box applies the same answer to all of them.
- Pasted into the folder they came from, copies become "name (2)"; a move does nothing.
- **Moving** on the same disk only changes the place, instantly. Between different devices EyeCanSee copies and deletes the original only after everything has been copied; the original does not go to the trash.
- A folder cannot be pasted inside itself, and nothing can be pasted into a trash or onto the list of a phone's storages.

## External devices

When you connect a phone, a camera, an external disk, a USB stick or an SD card, the **Devices** block appears at the top left, with its folder structure to expand like the tree. When you disconnect it, it disappears; if you were looking at it, EyeCanSee goes back to your home folder.

- **Android phone**: on the phone choose **Transfer photos** (PTP) or **Transfer files** (MTP). With Transfer photos you only see the photo folders (`DCIM`, `Pictures`); with Transfer files the whole storage.
- EyeCanSee connects to the phone directly, even if the desktop has already connected it (for example GNOME): this is much faster, especially in folders with hundreds of files. The device appears within a couple of seconds.
- On GNOME, opening the phone in EyeCanSee takes it away from the file manager. To get it back, close EyeCanSee and click the phone in the file manager, or disconnect and reconnect it.
- **"The device is in use by another application"**: another program holds the phone. If the desktop connected it, EyeCanSee switches to that connection by itself (slower: in folders with hundreds of files about one thumbnail per second); otherwise close the other program or disconnect and reconnect the phone.
- **"The device shows no storage"**: the phone was locked or set to "charging only" when it was connected: unlock it and choose File transfer. EyeCanSee retries by itself every 2 seconds and shows the storage as soon as it appears, no need for F5.
- **"The device was not found"**: the phone is reconnecting (it does so when you choose File transfer) or has been unplugged. EyeCanSee retries by itself every 2 seconds, no need for F5.
- When you open a directly connected phone, the block shows its folders and below there is an item to go back to your home folder.
- Thumbnails of devices stay in the cache after you disconnect them.
- External disks appear wherever the system mounts them. A partition the system did not mount by itself (for example because it has its own line in `/etc/fstab`) appears with a dimmed name: one click mounts and opens it. If needed, the system asks for your password.
- On Windows USB sticks, SD cards and disks connected via USB appear, with the name they have in File Explorer (for example "Backup (D:)"). Internal drives (C:, and any others) are under **This computer**: one click on another drive opens it. Phones connect through the Windows driver. Network drives and CDs/DVDs do not appear yet: open them with **File > Open location…** (for example `E:\`).

## Opening images

Type a folder, an image or a remote address in the address bar and press Enter (File > Open location focuses it), or start EyeCanSee with one:

```text
eyecansee                                  your home folder
eyecansee ~/Pictures/Holidays              a folder
eyecansee ~/Pictures/Holidays/beach.jpg    opens that image in the viewer
eyecansee ftp://user:password@nas.lan/Photos
eyecansee ftps://user@nas.lan/Photos
eyecansee sftp://user@server:2222/home/user/Photos   (Linux only)
```

Only one EyeCanSee runs at a time (a phone can be used by one program only): if it is already open, starting it again, for example with "Open with" on a photo, shows that folder or image in the window already open.

Remote folders behave exactly like local ones: the tree, the grid and the viewer work the same way. Passwords are never shown in the address bar.

With SFTP, EyeCanSee uses the system `ssh` client, so your `~/.ssh/config` aliases, keys and agent work as usual.

## Keyboard

In the grid:

| Key | Action |
|---|---|
| Arrows | Move the selection (the first press selects the first item; left from the first item goes to **..**) |
| Home, End | First, last item |
| Enter | Open the folder or the image; on **..** go up |
| Backspace | Parent folder |
| F2 | Rename (see *Deleting and renaming*) |
| Del | Delete |
| Ctrl+C, Ctrl+X, Ctrl+V | Copy, cut, paste (see *Copying and moving*) |
| Ctrl+A | Select everything in the folder |
| Shift+arrows | Extend the selection |
| Shift+Del | Delete for good |
| Tab | Next item; from the last one it moves to the devices |
| Shift+Tab | Previous item; from the first one it moves to the folder open on the left |

In the tree on the left:

| Key | Action |
|---|---|
| Down, up | Next, previous folder |
| Right | Expands the folder; if already expanded, goes to its first subfolder |
| Left | Collapses the folder; if already collapsed, goes to its parent |
| Backspace | Parent folder |
| Tab | Next folder; from the last one it moves to the grid |
| Shift+Tab | Previous folder; from the first one it moves to the last device |

In the devices: down and up change device (down on an expanded device enters its first folder), right and left expand and collapse it, Enter opens it on the right, Tab moves to the next one and from the last one to the computer's folders, Shift+Tab to the previous one and from the first one to the last item of the grid.

In the viewer:

| Key | Action |
|---|---|
| Right arrow, Space, Page Down | Next image |
| Left arrow, Backspace, Page Up | Previous image |
| Home, End | First, last image |
| Esc | Back to the grid |
| F2, Del, Shift+Del | Rename and delete, as in the grid |

Everywhere: **F1** opens this manual, **F5** reloads the folders. In the viewer F5 stays on the open image; if it has been deleted in the meantime, it goes back to the grid.

Folders on this computer update by themselves: images added, deleted, renamed or restored from the trash by another program appear and disappear without F5, also in the viewer, which stays on the open image. On phones and servers you need F5.

The same actions are in the menus: **View** (Open, Back to the grid, Refresh) and **Go** (Next, Previous, First, Last, Parent folder). In the grid Next and Previous move the selection, in the viewer they change image.

A **right click** opens a menu with the actions for what is under the mouse, and selects it. On an image or a folder (in the grid or the tree): Open, Cut, Copy (on a folder also Paste into this folder, when something has been copied), Rename, then, apart at the bottom, Delete and Delete for good. On the image open in the viewer the same, with Back to the grid instead of Open. On **..**: Parent folder. On an empty space of the grid: Paste (when something has been copied), Refresh and Parent folder (inside the Trash also Restore all and Empty the trash).

In the viewer, after the last image EyeCanSee stops; with the option **In the viewer, after the last image start again from the first** it goes back to the first one, and the other way round. EyeCanSee loads the images around the current one in advance, so moving between them is immediate even on slow connections.

## Supported formats

- **Photos**: JPEG, PNG, WebP, HEIC/HEIF, AVIF, JPEG XL, TIFF, GIF (first frame), BMP.
- **Camera RAW**: ARW, CR2, CR3, CRW, DNG, NEF, NRW, ORF, PEF, RAF, RW2, SRW, 3FR, IIQ, ERF, KDC, MEF, MOS, MRW and others. EyeCanSee shows the full-size preview stored by the camera; if there is none, it develops the sensor data.
- **Vector**: SVG, SVGZ.
- **Graphics and HDR**: PSD (merged image), ORA, OpenEXR, HDR, DDS, ICNS, QOI, TGA, ICO, CUR, PCX, JPEG 2000, PNM, XPM, XBM, SGI, WBMP, OTB, MPO, JPS.
- **Through ImageMagick, on Linux only**: PSB, XCF, PFM, Sun Raster, PICT, MIFF, MNG, JNG, DPX, Cineon, FITS and other less common formats. On Linux ImageMagick also opens the PSD files the built-in reader does not handle (for example 32-bit ones).

Photos are rotated automatically using their EXIF orientation. The file extension is only a hint: an iPhone HEIC photo renamed to `.jpg` opens anyway.

On Windows the formats handled by ImageMagick are not available: those files do not appear in the list.

## Language

EyeCanSee uses the system language when it is available, otherwise English. To force a language set `EYECANSEE_LANG`, for example `EYECANSEE_LANG=en eyecansee`.

## Known limits

- On **NTFS** disks (for example an external disk also used with Windows) the Files app (Nautilus) cannot restore from the trash, not even files it deleted itself; EyeCanSee reminds you in the confirmation. The files are in the trash all the same and can be restored from a terminal: `gio trash --list` shows what is there, `gio trash --restore "<trash:/// address from that line>"` puts it back.
- SFTP is not available on Windows yet.
- Images larger than 8192 pixels per side are shown downscaled.
- Files are sorted alphabetically: `img10` comes before `img2`.
- Hidden files and folders (starting with a dot, or on Windows with the "hidden" attribute) are
  not shown.
- HDR images (EXR, HDR, PFM) are shown without tone mapping and may look overexposed.
