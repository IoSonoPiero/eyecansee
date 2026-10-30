# EyeCanSee manual

Press **F1** at any time to open or close this manual.

## The window

- **Menu bar**: File, Edit, Image, View, Tools, Help. The version number is on the right; **Help → About EyeCanSee** shows the version, the author, the license and the link to the source code. Every action you can do with the keyboard or a double click also has a menu item, with its key shown next to it.
- **Toolbar**: parent folder, refresh, metadata (ℹ), the address bar, the image order and the thumbnail size slider.
- **Folder tree** on the left, under **This computer** (and under **Devices** if you have any connected): click a folder to show its content. Folders open only when you expand them, so remote sources stay fast. When you change folder, also from the grid, the tree opens down to that folder and highlights it.
- **Grid** on the right: the **..** cell to go up one level (not at the top of a disk or device), the subfolders, then the images as thumbnails, spread over the whole width. When you enter a folder nothing is selected; when you go up, the folder you came from is selected.
- **Image order** from the drop-down list in the toolbar (with the ▲/▼ button next to it to reverse it) or from **View → Sort by**: by name, modification date, size or type (extension), plus **Descending order**. Folders stay first, by name; files without a date stay at the end. The choice is remembered.
- **Metadata** below the grid, for the selected image: boxes **Camera** (make, model, lens, software), **Shot** (date, exposure, aperture, ISO, focal length, flash), **Image** (format, size, colour and bits per channel, colour space; format, size and colour also for images without EXIF) and **Location** (GPS coordinates, altitude, with a link to open them on the OpenStreetMap map). Each box closes with its X and reopens from **View → Metadata boxes**; the ℹ button in the toolbar closes or reopens them all. The panel collapses and reopens with the ▼/▲ button at the right of its bar, or with a click on the bar itself; when collapsed only the bar stays, and the metadata is not read. EXIF metadata is read from JPEG, HEIC, AVIF, PNG, WebP and TIFF; to read it EyeCanSee reads the file again, so on a phone it takes a moment; once read, it is kept with the thumbnails and shows at once the next time (until the file changes; emptying the thumbnail cache empties it too).
- **Status bar** at the bottom: how many folders and images there are, and name, size and modification date of the selected item; with several items selected, how many and the size of their images.
- **Several items**: Ctrl+click adds or removes an item, Shift+click selects everything from the last clicked item to this one, Shift+arrows extend the selection, Ctrl+A selects everything in the folder. **Dragging** with the mouse draws a rectangle that selects the cells it touches (with Ctrl it adds them to those already selected); near the top or bottom edge the grid scrolls. A click without keys, or an arrow, goes back to a single item.
- Arrows and Tab act on the side you clicked last, left or right: it is the one with the thin frame. There the selection is solid blue, on the other side it is lighter.

Double-click an image, or select it and press Enter, to open it in the **viewer**. Press Esc, double-click the image or use the grid button in the toolbar to go back to the grid. The bar at the bottom of the viewer shows the file name, its position in the folder (for example 12 / 51), its size in pixels and on disk, and the date it was taken (the date it was modified when the photo has no such information or the metadata boxes are closed).

## Thumbnails

EyeCanSee creates the thumbnails in this order: the selected image first, then the ones visible in the window from top to bottom, then all the others in the folder from top to bottom. Clicking an image moves its thumbnail to the front of the queue. Up to 1000 thumbnails per folder stay in memory, so they do not disappear while scrolling, even when dragging the scrollbar; in bigger folders the ones far from the visible part are reloaded from disk.

Thumbnails are saved on disk, so opening the same folder again is immediate: in the file `~/.cache/eyecansee/thumbs.db` on Linux and `%LOCALAPPDATA%\eyecansee\thumbs.db` on Windows. If an image changes (size or modification date), its thumbnail is created again. You can delete that file (with EyeCanSee closed) at any time: thumbnails are created again when needed.

**Clean up the thumbnail cache** in **Tools > Options…** (off by default): once per session, for each opened location, removes thumbnails of deleted images and recreates those of changed images; then removes the least recently used thumbnails until the cache is within **Cache limit** (default 1024 MB). Folders that cannot be found or reached are skipped, so a disconnected disk or a switched-off NAS does not lose its thumbnails. Remote locations are checked only when you open them.

## Options

**Tools > Options…** opens the options window, divided into tabs: **General** (last folder, window at startup, language, updates), **Look** (colors), **Browsing** (scrolling, viewer), **Deleting**, **Thumbnails**, **Slideshow** and **Video** (video engine and FFmpeg). Every change is saved immediately in `~/.config/eyecansee/settings.json` on Linux and `%APPDATA%\eyecansee\settings.json` on Windows, a text file you can also edit by hand.

- **Reopen the last folder at startup** (on by default): when EyeCanSee starts without a folder, for example from the applications menu, it opens the folder you were viewing when you closed it. If that folder no longer exists, it opens your home folder. Remote locations reopen with the password only when the server is among the **saved servers** (see *Opening images*): otherwise, for an FTP server with a user, **File > Open location…** opens with everything filled in except the password, ready to be typed (Cancel stays in your home folder).
- **Remember the scroll position of each folder** (off by default): when you go back to a folder you already viewed, the grid starts where you left it. When off, every folder you open starts from the first thumbnail. It lasts until EyeCanSee is closed.
- **In the viewer, after the last image start again from the first** (off by default): and from the first image, going back shows the last one. When off, the viewer stops at the first and the last image.
- **Mouse wheel in the viewer**: **Zooms in and out** (default) or **Shows the next or previous image**. Holding Ctrl, the wheel always zooms.
- **Largest zoom in the viewer** (2000% at first): how far the wheel and + can enlarge an image.
- **Window at startup**, one choice only: **Normal size**, **Maximized** (fills the screen, leaving the system bars visible), or **Size it had when closed** (if it was maximized, it reopens maximized).
- **Check for updates at startup** (on by default): see *Updates*.
- **Language**: **Same as the system** (default: the Windows or desktop language, English if it is not among the available ones) or one of the listed languages, each shown by its own name. The choice applies at once, this manual included.
- **Colors**: **Same as the system** (default: light or dark like the Windows or desktop theme, following it when it changes), **Light** or **Dark**. The choice applies at once.
- **Clean up the thumbnail cache** and **Cache limit**: see *Thumbnails*.
- **Empty the thumbnail cache…**: deletes all saved thumbnails, after a confirmation showing how many and how much space is freed. Your images are not touched; thumbnails of the open folder are created again right away, the others when you open them again.
- **Delete files on this computer for good, without the trash (dangerous)** (off at first): see *Deleting and renaming*.
- **Deleting on phones and servers**: see *Deleting and renaming*.
- **Slideshow**: how long each image stays, the passage between images and whether it starts again after the last one: see *Slideshow*.
- **Video engine (libmpv) and FFmpeg to use** (Video tab): **The system's, if recent enough; otherwise the downloaded one** (default), **Always the system's, whatever its version**, or **Always the downloaded one**. EyeCanSee needs at least libmpv 1.109 (mpv 0.33) and FFmpeg 4.3; on Linux the distribution's packages come first because they are better integrated with the graphics card. The tab also shows which ones are in use now, with their versions. A change applies the next time EyeCanSee starts. See *Videos* and *Converting and resizing images and videos*.

## Updates

The Windows installer, the Windows zip and the AppImage update themselves. At startup EyeCanSee asks GitHub, in the background, for the latest published version; if it is newer, it downloads it and checks it twice: against the SHA-256 fingerprint GitHub publishes for it (a broken download), and against the developer's signature, made with a key that never leaves the developer's PC (a file that does not come from the developer). A file without a valid signature is never installed, and Check for updates says why. When it is ready, a red **Install … and restart** button appears at the top right of the menu bar: EyeCanSee closes, installs the new version and opens again. Nothing is installed until you click it.

- **Help > Check for updates** does the same check at any time and tells you the outcome.
- The check can be turned off in **Tools > Options… > General**. When off, EyeCanSee contacts GitHub only when you choose Check for updates. Nothing about you or your images is sent: only the request for the latest version.
- On Fedora, EyeCanSee is updated by `dnf` with the rest of the system.

## Deleting and renaming

**File > New folder…**, **New folder…** from the right click on an empty spot of the grid, or **Ctrl+Shift+N** makes a folder in the folder open: the name comes already written ("New folder", with " (2)" when taken) and selected, so you can type the one you want; **Create** makes it and selects it in the grid. From the right click on a folder (in the grid or the tree), **New folder inside…** makes it inside that folder. It works on the computer, on disks, phones and servers; not in a trash nor among the storages of a phone.

**Del** (or **File > Delete…**) deletes the image or folder selected in the grid, or the image open in the viewer, after a confirmation showing its name and size. A folder is deleted with everything inside it. Folders in the tree on the left are deleted with a right click. In the confirmation **Enter** deletes and **Esc** cancels. In the viewer the next image appears at once.

With several items selected, **Del** (or **Delete…** from the right click on one of them) deletes all of them, after a confirmation with how many they are and the size of the images. If one fails, the following ones are not deleted. While a deletion runs the status bar says so, with a spinner, and no other deletion can be started (on a phone a folder with hundreds of files takes several seconds).

- **Files on this computer and external disks** go to the system trash, where they can be restored. On Windows, USB sticks, SD cards and network drives have no Recycle Bin: there the image goes into the hidden `.EyeCanSee-trash` folder next to it, as on phones (see below), and the confirmation says so. With the option **Delete files on this computer for good, without the trash** they are deleted forever.
- **Phones and servers (FTP, SFTP)** have no trash EyeCanSee can reach. In the Options, under **Deleting on phones and servers**, choose between **Move to the hidden .EyeCanSee-trash folder next to the image** (default: the image goes into a `.EyeCanSee-trash` folder inside its own folder, and you get it back with a file manager) and **Delete for good**. FTP cannot move files: there you need **Delete for good**.

**Shift+Del** (or **File > Delete for good…**) deletes forever that one time, whatever the option says.

**F2** (or **File > Rename…**) renames the selected item or the open image: type the new name and press **Enter** (**Esc** cancels). EyeCanSee does not accept a name already in the folder. Over FTP nothing can be renamed.

The top folder of a source and the storages of a phone cannot be deleted or renamed.

**The trash of phones and servers** (and of USB sticks on Windows): every folder where you deleted something has a **Trash**: a strip at the top of the grid says so, with the **Open the trash** button. Inside it the strip says whose trash it is and has **Restore all** and **Empty the trash…**. Inside it you can look at the images, **restore** them (they go back to the folder containing the trash) or delete them for good: there **Del** always deletes forever, and there is no Rename. Right click on an empty space inside the Trash: **Restore all** and **Empty the trash…** (everything deleted forever, after a confirmation). A trash left empty, after restoring or deleting what was inside it, goes away together with its strip. An empty `.EyeCanSee-trash` folder (copied from another device, for example) shows no strip. On Android phones the trash of a folder also shows the photos and videos the phone's gallery put in its own trash (hidden `.trashed-…` files, which Android deletes by itself after 30 days: the scheme of Google Photos and Files by Google; the Xiaomi Gallery keeps its own bin elsewhere, which EyeCanSee does not see): they appear with their real name, are restored like the others (back to their name in the same folder) and deleting them there deletes them for good; **Empty the trash…** deletes them too and removes the `.EyeCanSee-trash` folder, **Restore all** renames them all and removes the folder if it is left empty.

If the name of an item being restored is already taken, EyeCanSee asks what to do: for a file **Replace** (the one already there is deleted) or **Keep both** (the restored one becomes for example `photo (2).jpg`); for a folder **Merge** (the contents add up; files with the same name are both kept) or **Keep both**. **Cancel** also stops the rest of a Restore all. In the trash too a name already there becomes `photo (2).jpg`.

## Finding duplicates

**Tools > Find duplicates…**, or **Find duplicates…** from the right click on a folder (or on an empty space of the grid, for the open folder), looks for identical images: the same file byte for byte, even under another name or in another folder. Images that were resized or saved again are not identical and are not found. It works on every source: computer, external disks, phones and servers.

First choose whether to look **Also in subfolders** (on by default), then **Search** (**Enter**; **Esc** cancels). A window shows how many images have been examined out of those found and how much has been read; **Cancel** stops the search. Only images with the same size are read and compared, so on a computer it is quick; on a phone every image that could be a copy is read in full, and it takes time.

The results fill the window. Every group of identical images has its thumbnail (click it to open the image in the viewer; **Esc** brings you back to the results) and one line per copy, with its date and its path below the folder searched (the full path when you point at it). The box on each line means **delete this copy** (a click on the text of the line ticks or unticks it too): the copies to delete are faded and struck through, the one that stays says **Stays**. At least one copy of every group always stays: the last box left unticked cannot be ticked.

At the start every group keeps its first copy. **Order** decides which one is first, for all the groups: **Date, oldest first** (to the minute, as shown; with the same date, the shorter path), **Shortest name** (copies usually have something added to the name, such as "photo copy.jpg" or "photo (2).jpg": so the original stays even when the dates are the same; when the names are as long, as Date) or **Path** (alphabetical order of folder and name). **Keep the first**, **Keep the last** and **Keep all** mark every group at once; then you can change single boxes. The line at the bottom says how many groups there are, how many copies will be deleted and how much space is freed. **Delete the marked copies…** asks for confirmation and deletes them as **Del** does (trash, the folder's Trash or for good, as in the Options). **Close** or **Esc** closes the results.

With the keyboard: **↑** and **↓** move between the copies of all the groups, **Space** ticks or unticks, **Esc** closes. Deleting is done with the **Delete the marked copies…** button: Enter does not delete.

## Saving in another format

Right-click an image (in the grid or in the viewer) and choose **Save as…**. A page takes the whole window: the original on the left, the converted image on the right, each with its size in pixels and on disk (for the converted one also as a percentage of the original). What you see on the right is the file as it will be saved: it is encoded and decoded back every time you change an option. While that happens the size says "Working out the size…" and Save waits: the size shown is always that of the file that will be saved.

- **Format**: JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF, QOI, ICO, TGA, PNM, OpenEXR, Radiance HDR or Farbfeld, with all the options of each one: quality, colour detail, progressive and lossless JPEG; compression level and filter for PNG; lossy or lossless WebP; quality, speed and bit depth for AVIF; compression for TIFF; colour reduction for GIF; compression for TGA. An icon (ICO) holds at most 256 × 256 pixels, so a larger image is reduced; JPEG and HDR have no transparency, so transparent parts become white: the page says so. These six formats are only for Save as: Save writes back over the original only the first eight. The full table is in [docs/formats.md](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/formats.md).
- **100%** shows the real pixels; drag either image to move both, so you compare the same spot.
- **Keep the metadata (EXIF, colour profile)** is on at first. It is greyed out when the original has none or the format cannot hold them. The EXIF saved is what the metadata panel shows, also from RAW and HEIC photos (camera, shot, GPS; not the maker's private notes). The orientation is set to upright, since the saved pixels are already turned.
- **File name** and **Save**: the new file goes into the same folder as the original. If the name is already taken, " (2)" is added, and the page says so before you save.
- Esc or ✕ closes the page without saving.

## Converting and resizing images and videos

Select images and videos in the grid (or right-click one of the selected ones) and choose **Tools > Convert and resize…** or **Convert and resize…** from the right-click menu. A window asks:

**Tools > Copy with the metadata…** (in the right click too, when a video is among the files chosen) opens the same window already set on **Same as the original**: the video is rewritten without re-encoding, in seconds, with its metadata inside (for dashcams and drones the position taken from the files beside the video too); you only choose where to put it.

- **Format**: one of those of *Save in another format*, with its options under **Options**, or **Same as the original**: each image in its own format, written as Save writes it (JPEG at quality 95); images in a format EyeCanSee cannot write (HEIC, RAW…) are then left out.
- **Videos** (only when videos are among the files chosen): **container** (MP4, MKV, WebM, MOV, AVI, GIF), **codec** (H.264, HEVC, AV1, VP9, VP8, MPEG-4, ProRes, GIF, or **Same as the original**: video and sound copied without re-encoding, in seconds and without loss, with the metadata written into the new file; the turn and the size do not apply) and **encoder** (those of the graphics card in green, with a line that says so), with a line on what the one chosen is good for; **quality** (High, Medium, Small file); **Keep the sound**; **Keep the metadata** (date, place, camera; for dashcams with `gps.sqlite` and drones with `.SRT` the new file gets the starting point, and for dashcams the altitude and average and top speed too, so the panel shows them away from the card as well). Videos are converted by FFmpeg: the system's one when there is one and it is at least version 4.3 (an older one is reported with its version, and the download offered), otherwise the window offers to download it (63 MB on Windows, 67 MB on Linux, once, into the app's data folder; its SHA-256 is checked). The first time EyeCanSee tries every encoder and shows only those that work on that computer: first the graphics card ones (NVIDIA, Intel Quick Sync, AMD, Windows, VA-API on Linux), much faster, then the processor ones, slower but with slightly smaller files at the same quality. Codecs without a working encoder stay grey. The full table is in [docs/video-formats.md](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/video-formats.md). Videos on a phone or a server are copied into a temporary file first.
- **Turn and flip**: a quarter turn, half a turn, and horizontal or vertical flips; the flips come first, then the turn. Only turned and in their own format, JPEGs are turned without losing quality when their size allows it, as with Save.
- **Size**: unchanged, a percentage of the original, or inside a box of pixels keeping the proportions (after the turn). Smaller files are not enlarged unless you ask. For images, the **filter** decides how the pixels are worked out: Lanczos is the sharpest, Nearest pixel keeps the hard edges of pixel art.
- **Where**: next to the originals, in a subfolder (created if missing), or in another folder, also on another device: **Browse…** opens an EyeCanSee window to choose the folder, with the home folder, the drives and the connected devices (phones too) on the left: a click on a folder goes into it, **Use this folder** chooses it, or you write an address as in the address bar (`mtp://…` for a phone, `sftp://…`); that folder must exist. So photos of the phone can be converted straight into a folder of the computer. The originals are never touched; a name already taken gets " (2)".

**Convert** starts the work; a box in the corner shows how far it got, and the program can be used meanwhile. The bar moves during a long video too. **Cancel** stops at once (the half-made video is deleted). At the end a message says how many files were saved and where, and which were left out and why: animated images and images with several pages (they would keep only the first one). Phone photos with an HDR map or a motion photo video are converted: the new file is the picture, the original keeps the rest. It works on every device: computer, disks, phone.

## Slideshow

**View > Slideshow**, or **Slideshow** in the right-click menu of an image (in the grid and in the viewer), shows the images one after the other at full screen, on black. With several images selected it shows those, always from the first in the grid; otherwise every image of the folder, from the chosen one (or the one open in the viewer).

- **Esc** or a double click ends it: you are back where you were, on the last image shown. **Space** pauses and goes on. **←** and **→** (or Page Up and Page Down) show the previous and next image at once; with **Slide**, going back the images slide the other way.
- In **Tools > Options… > Slideshow**: how many seconds each image stays (5 at first), the passage between images (**None**, **Crossfade**, the default, **Fade through black**, **Slide**, **Zoom and fade**), and **Start again after the last image** (on at first; off, the slideshow ends after the last one).
- The next image is read in advance: if it is not ready yet (a phone, a large file) the current one stays until it is.

## Videos

Videos (MP4, MOV, MKV, WebM, AVI and others) appear in the grid next to the images, with a frame of the video as thumbnail and a ▶ badge. When the camera stored a preview inside the video (dashcams such as Umino do, and DJI drones as a cover) the thumbnail is that preview, ready almost at once. They are played by mpv. On Linux EyeCanSee uses the system's libmpv when there is one (`mpv-libs` on Fedora, which the Fedora package asks for by itself, `libmpv2` on Debian, Ubuntu and openSUSE, `libmpv1` on Ubuntu 22.04, `mpv` on Arch) and it is recent enough: libmpv 1.109, that is mpv 0.33 or later (an older one is reported with its version, and the download is offered). Otherwise, and always on Windows, the viewer offers to download the video engine the first time a video is opened (10 MB on Linux, 47 MB on Windows, once; it goes in the app's data folder and its SHA-256 is checked before use), then EyeCanSee restarts to activate it. Which one to use (the system's, the downloaded one, or the system's only when recent enough) is chosen under *Options*, Video tab. There is no need to open a video: as soon as the open folder holds a video and the engine is missing, the red **Download the video engine** button appears at the top right (as the update one does), downloads it and then turns into **Restart EyeCanSee**.

- **Enter** or a double click opens the video in the viewer, where it starts playing with sound, fitted to the window.
- **Space**, a click on the video or the button under it play and pause. The bar under the video shows the time and moves along it: drag it or click a point. At the end the last frame stays; play starts again from the beginning.
- The volume is at the right of the bar: the slider (0 to 100) and the button that turns the sound off and on. **↑** and **↓** raise and lower it by 5, and so does the mouse wheel over the video; **M** turns the sound off and on. Volume and sound off stay for the next videos and the next time EyeCanSee starts. On a video without sound the two controls are greyed out.
- **←** and **→** go to the previous and next item, as for images; a double click or **Esc** goes back to the grid.
- The metadata panel shows a selected video's details when the file has them: make, model and Android version of the phone, date and time of recording, length, frame rate, size, video and audio format, bit rate, and the location (phones and iPhones write it). Dashcams that keep their GPS track in a `gps.sqlite` file on the card (such as Umino) show where the video starts, the altitude, and the average and top speed during the video; a copy of the video made by hand without that file has no location (Convert and resize writes it into the new file). DJI drones write the position in a `.SRT` file with the same name as the video: the panel shows where the video starts, taken from its first frame; if the drone had no GPS fix yet when recording started, the video has no location rather than a wrong one.
- **View > Show**, or **Show** in the right-click menu of the empty part of the grid: **Images and videos** (the default), **Images only** or **Videos only**. The choice stays for every folder until you change it; the status bar counts images and videos apart.
- Videos on a phone or a server are read a piece at a time while they play, never copied whole. Copying, moving, renaming and deleting work as for images. Save and the turns in the viewer are for images only: videos are turned and converted from *Convert and resize*. In the slideshow a video plays whole, with sound, then the next item comes; **Space** pauses it too.

## Rotating and flipping

In the viewer, the **Image** menu flips the image horizontally or vertically and rotates it by 90° clockwise (Ctrl+R) or anticlockwise (Ctrl+Shift+R) or by 180°; the two rotations are also buttons on the toolbar, and everything is in the right-click menu. The change is only shown until you save it (the status bar says "Changed, not saved", in yellow). Leaving the image with the change not saved (another image, back to the grid, the parent folder, closing EyeCanSee) first asks whether to **Save**, **Save as…** or **Don't save**; **Cancel** stays on the image. Rotate and crop asks the same way.

- **File > Save** (Ctrl+S) saves over the original, after asking: first it prepares the new file (the status bar says "Preparing to save…"), so the question can say whether anything is lost; the original goes where a deletion would send it (the trash, usually). A JPEG is rotated without losing quality, as `jpegtran` does, when its width and height are multiples of 16 pixels (8 for some); otherwise it is encoded again at quality 95. PNG, WebP, AVIF, TIFF, BMP, GIF and QOI are encoded again in their own format. The confirmation says when the image will be encoded again with some loss of quality (a JPEG of other sizes, lossy WebP, AVIF, GIF). A PSD is saved as a PSD, with its colour profile, EXIF and XMP: one with a single plain layer loses nothing, one with several layers, text, effects or masks is **flattened** (only the image as it looks remains), and the confirmation says so in bold. The formats EyeCanSee cannot write (HEIC, RAW…) can only use Save as, and so can images that would lose something: animated PNG, WebP, AVIF and GIF, TIFF files with several pages, and JPEG photos carrying more than one picture (the video of a motion photo, depth maps). **Ultra HDR** phone photos (Android 14 and later) are flipped and turned like the others instead: EyeCanSee turns their HDR map too, so they stay HDR. On FTP servers images cannot be saved over themselves: renaming is not possible there. When an image cannot be saved over itself, Ctrl+S says why in a window with a **Save as…** button.
- The new file is written under a hidden name first, and only takes the image's name once it is complete; if something goes wrong, or the program closes half way, the original stays (or comes back, the next time the folder is opened) under its name. If the program closes just after the new file has taken the name, a hidden file `.<name>.eyecansee-old` may stay next to the saved image: it is the original. The next Save of that image sends it where a deletion would; until then you can delete it or rename it yourself.
- **File > Save as…** (Ctrl+Shift+S) opens *Save in another format* with the image already rotated: the original is not touched.

In the grid the same operations (the Image menu, or right-click > Transform) work on the selected image, or on all the marked ones, and save them straight away: EyeCanSee asks first, says where the originals go and warns that lossy formats may lose a little quality; images whose format cannot be saved back are left out, and so are animated and multi-page images and motion photos, listed by name in the window that says, at the end, how many images were saved. Their thumbnails are then made again. When the only image chosen cannot be saved back (a PSD, a RAW…), the window says why and offers **Save as…**, which opens *Save in another format* with the image already turned.

### Rotate and crop

**Image > Rotate and crop…** (also in the right-click menu) opens a page of its own for one image, to rotate it by any angle and crop it, as photo editors do. With several images selected it is greyed out: to turn several images together use the flips and turns above it or **Convert and resize**.

- **Rotation**: the slider or the number next to it, from -180° to 180° by tenths of a degree; −90° and +90° add a quarter turn, **Back to 0°** undoes it. With Shift held, the mouse wheel on the image turns it by 0.1° a notch: handy for straightening a horizon against the thirds lines. The degrees can be typed with a dot or a comma.
- **Straighten**: click it, then draw a line along the horizon (or along something upright, such as a wall): when you let go the image turns to make that line level (or upright), and the button turns itself off.
- **Crop**: drag a corner or a side of the frame to resize it, its inside to move it. The proportions can be free, as the original, 1:1, 5:4, 4:3, 3:2, 16:9 or your own; ⇄ swaps width and height. **Image only** makes the largest crop with no empty corners (it follows the rotation, and it is what you get at first); **Whole image** keeps all of the rotated image.
- **Empty corners**: where the crop goes past the image they are transparent (grey squares) or filled with the colour you choose. JPEG has no transparency: there they become white.
- The wheel zooms around the pointer, dragging outside the frame moves the view, a double click shows everything again. The size of the result is under the controls.
- **Apply** (or Enter) puts the rotation and the crop into the image shown, and the controls start again from it: you can rotate, crop and apply again as many times as you like. The file is not touched: under Apply the page says "Rotation and crop not applied yet" or "Changes applied, not saved yet".
- At the end, **Save…** writes the result over the original after asking, like Save in the viewer ("Preparing to save…" next to the buttons until the new file is ready; same formats, same exclusions, the original goes where a deletion would send it); the image is encoded again, so JPEG loses a little quality. **Save as…** opens *Save in another format* with the result. **Close**, ✕ or Esc close the page; with changes not saved they first ask whether to **Save**, **Save as…** or **Don't save** (**Cancel** goes back to the page).
- **Ultra HDR** photos of phones (with HDR brightness) stay HDR: their HDR part is rotated and cropped together with the photo.

## Copying and moving

**Ctrl+C** copies the selected items (or the image open in the viewer), **Ctrl+X** cuts them; then open the folder where they should go and press **Ctrl+V**. Each copy or cut is pasted once: to paste again, copy again. The same items are in the **File** menu and in the right click; the right click on a folder also has **Paste into this folder**. Files and folders, with everything inside them, go anywhere: another folder of the same disk, another disk, a phone, a server, also from one device to another.

- Copied items get a blue badge in the corner of their cell, cut items an orange one with scissors and fade; the status bar says how many are waiting for **Ctrl+V**.
- A window at the bottom right shows the file being copied, the destination folder, how many files and how much data have been copied, the percentage, the speed and the time left, with **Cancel**: the file being copied when you cancel is not left halfway, the ones already copied stay. Towards a phone, Cancel waits for the end of the file being copied (a phone receiving half a file stops answering). Closing the window during a copy cancels it, and the window closes once the file being copied is finished.
- If an item with the same name is already there, EyeCanSee asks: **Replace** (for a folder **Merge**: its contents go into the existing folder, replacing the files with the same name), **Skip** or **Keep both** (the new one becomes "name (2)"). With several such items, a check box applies the same answer to all of them.
- Pasted into the folder they came from, copies become "name (2)"; a move does nothing.
- **Moving** on the same disk only changes the place, instantly. Between different devices EyeCanSee copies and deletes the original only after everything has been copied; the original does not go to the trash.
- A folder cannot be pasted inside itself, and nothing can be pasted into a trash or onto the list of a phone's storages.

## External devices

When you connect a phone, a camera, an external disk, a USB stick or an SD card, the **Devices** block appears at the top left, with its folder structure to expand like the tree. When you disconnect it, it disappears; if you were looking at it, EyeCanSee goes back to your home folder.

- **Android phone**: on the phone choose **Transfer photos** (PTP) or **Transfer files** (MTP). With Transfer photos you only see the photo folders (`DCIM`, `Pictures`); with Transfer files the whole storage.
- EyeCanSee connects to the phone directly, even if the desktop has already connected it (for example GNOME): this is much faster, especially in folders with hundreds of files. The device appears within a couple of seconds.
- On GNOME and KDE, opening the phone in EyeCanSee takes it away from the file manager On KDE EyeCanSee asks first: to take the phone it has to close KDE's kiod service, which also forgets the passwords of network folders open in Dolphin (it asks for them again next time). With Cancel the phone stays with KDE; F5 asks again. If you also open the phone in Dolphin while EyeCanSee has it, KDE gives up on it and does not see it again: when you close EyeCanSee or go to another folder, EyeCanSee puts KDE right by itself (if you said OK). Otherwise disconnect and reconnect the phone. To get it back, close EyeCanSee and click the phone in the file manager, or disconnect and reconnect it.
- **"The device is in use by another application"**: another program holds the phone. If the desktop connected it, EyeCanSee switches to that connection by itself (slower: in folders with hundreds of files about one thumbnail per second); otherwise close the other program or disconnect and reconnect the phone.
- If the direct connection fails for a reason of its own and the desktop had already connected the phone, EyeCanSee opens it through that connection and says so in the status bar ("Opened through the file manager: slower"), with the error of the direct one. Without that connection the error is shown and EyeCanSee retries every 2 seconds.
- **"The device shows no storage"**: the phone was locked or set to "charging only" when it was connected: unlock it and choose File transfer. EyeCanSee retries by itself every 2 seconds and shows the storage as soon as it appears, no need for F5.
- **"The device was not found"**: the phone is reconnecting (it does so when you choose File transfer) or has been unplugged. EyeCanSee retries by itself every 2 seconds, no need for F5.
- When you open a directly connected phone, the block shows its folders and below there is an item to go back to your home folder.
- Photos taken, deleted or moved on the phone itself while its folder is open appear and disappear by themselves, a few seconds after the phone has finished (most phones say so to EyeCanSee). If the file was open in the viewer, the viewer closes and a message says which file was removed. A phone that does not say so: EyeCanSee finds out at the first read (opening the file, say), and the same happens.
- **Windows**: right after a photo is taken, or a file deleted, on the phone, Windows can stop talking to the phone for about six minutes: after 15 seconds a message says so, and closes by itself when the phone answers again. It is Windows' own driver. Unplugging the cable and plugging it in again (then File transfer) frees it at once. More in [Windows stops talking to the phone](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.en.md).
- Thumbnails of devices stay in the cache after you disconnect them.
- External disks appear wherever the system mounts them. A partition the system did not mount by itself (for example because it has its own line in `/etc/fstab`) appears with a dimmed name: one click mounts and opens it. If needed, the system asks for your password.
- On Windows USB sticks, SD cards and disks connected via USB appear, with the name they have in File Explorer (for example "Backup (D:)"). Internal drives (C:, and any others) are under **This computer**: one click on another drive opens it. Phones connect through the Windows driver. Network drives and CDs/DVDs do not appear yet: open them with **File > Open location…** (for example `E:\`).

## Opening images

Type a folder, an image or a remote address in the address bar and press Enter, or start EyeCanSee with one. **File > Open location…** opens a window where you type the whole address, or choose the protocol and fill in server, port, user, password and folder (without a user, FTP logs in as `anonymous`):

```text
eyecansee                                  your home folder
eyecansee ~/Pictures/Holidays              a folder
eyecansee ~/Pictures/Holidays/beach.jpg    opens that image in the viewer
eyecansee ftp://user:password@nas.lan/Photos
eyecansee ftps://user@nas.lan/Photos
eyecansee sftp://user@server:2222/home/user/Photos
eyecansee sftp://my-nas/                   an alias of ~/.ssh/config
```

**Saved servers**: in the same window fill in the fields, give a **Name** (empty = the server's name) and press **Save server**. From then on the server is in the **Saved servers** list at the top of the window: choosing it fills in the fields, which you can change and save again with **Save server**; **Delete server** removes it, after a confirmation. For SFTP there are also **Key file** (**Browse…** opens the system window in the `~/.ssh` folder, or in your home folder without it) and **Key passphrase**: that key is tried first. An address that matches a saved server (same protocol, server, port and user), also typed in the address bar or reopened at startup, uses its password and key. ⚠️ For now passwords and passphrases are written **in plain text** in the `servers.json` file of the settings folder (on Linux readable only by you): do not save there passwords you do not want to leave on the computer.

Only one EyeCanSee runs at a time (a phone can be used by one program only): if it is already open, starting it again, for example with "Open with" on a photo, shows that folder or image in the window already open.

"Open with" offers EyeCanSee for images and for the common video files (MP4, MKV, MOV, WebM, AVI): on Windows the installer registers them, on Linux the `.desktop` file does. It never makes itself the default program: that choice stays yours.

Remote folders behave exactly like local ones: the tree, the grid and the viewer work the same way. Passwords are never shown in the address bar.

With SFTP, on Linux and Windows, EyeCanSee reads `~/.ssh/config` as ssh does (aliases, HostName, User, Port, IdentityFile; not ProxyJump) and logs in with the keys of the SSH agent, with key files without a passphrase, or with the password: if the server refuses, **File > Open location…** opens and asks for it. A password typed in the address is tried first. An address without a folder opens in the folder the server starts in (usually your home). The first time you connect to a server, EyeCanSee shows its key fingerprint and asks whether to trust it; once trusted it goes into `~/.ssh/known_hosts`, as ssh does. A server whose key has changed is refused. EyeCanSee keeps one connection per server, and renames and moves (folders too) are done by the server itself.

## Keyboard

In the grid:

| Key | Action |
|---|---|
| Arrows | Move the selection (the first press selects the first item; left from the first item goes to **..**) |
| Home, End | First, last item |
| Enter | Open the folder or the image; on **..** go up |
| Backspace | Parent folder |
| F2 | Rename (see *Deleting and renaming*) |
| Ctrl+Shift+N | New folder (see *Deleting and renaming*) |
| Del | Delete |
| Ctrl+C, Ctrl+X, Ctrl+V | Copy, cut, paste (see *Copying and moving*) |
| Ctrl+A | Select everything in the folder |
| Shift+arrows | Extend the selection |
| Shift+Del | Delete for good |
| Shift+F5 | Redo all the thumbnails of the folder |
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
| Right arrow, Space, Page Down | Next image (on a video Space plays and pauses) |
| Left arrow, Backspace, Page Up | Previous image |
| Home, End | First, last image |
| Esc | Back to the grid |
| F2, Del, Shift+Del | Rename and delete, as in the grid |
| Up arrow, Down arrow (or the mouse wheel) | On a video: volume up and down |
| M | On a video: sound off and on |
| + and − (or the mouse wheel) | Zoom in and out |
| 0 | Fit the image to the window |
| 1 | 100%: one image pixel per screen pixel |
| Ctrl+R, Ctrl+Shift+R | Rotate 90° clockwise, anticlockwise |
| Ctrl+S, Ctrl+Shift+S | Save, Save as |

The mouse wheel zooms around the pointer, + and − around the middle of the window; Ctrl and the wheel always zoom, also when the wheel is set to change image (see *Options*). A zoomed image is moved by dragging it. It never gets smaller than fitted to the window, nor larger than the largest zoom set in the options (2000% at first); the status bar shows the zoom. The next image starts fitted again.

A thumbnail is made once and kept: it is made again only when the file changes size or date. If one looks wrong (for example made by an older version), right-click the image, or select several, and choose **Redo thumbnails** (also in the View menu). **Shift+F5** redoes all the thumbnails of the open folder.

Everywhere: **F1** opens this manual, **F5** reloads the folders. In the viewer F5 stays on the open image; if it has been deleted in the meantime, it goes back to the grid.

Folders on this computer update by themselves: images added, deleted, renamed or restored from the trash by another program appear and disappear without F5, also in the viewer, which stays on the open image. On phones too, when the phone says what changed (see External devices); on servers you need F5.

The same actions are in the menus: **View** (Open, Back to the grid, Next, Previous, First, Last, Parent folder, Refresh) and **Edit** (Cut, Copy, Paste, Select all). In the grid Next and Previous move the selection, in the viewer they change image. With several images selected, Enter or Open (also from the right click) open the first of them in the grid, and the viewer moves only among them, as the slideshow does, and the title counts only them.

A **right click** opens a menu with the actions for what is under the mouse, and selects it. The items follow the order of the menu bar. On an image: Open, Slideshow, Redo thumbnails; Transform, Save as…, Convert and resize…; Cut, Copy; Rename; then, apart at the bottom, Delete and Delete for good. On the image open in the viewer the same, with Back to the grid instead of Open and Save when you changed it. On a folder (in the grid or the tree): Open, New folder inside…; Cut, Copy (and Paste into this folder, when something has been copied); Find duplicates…; Rename; Delete and Delete for good. On **..**: Parent folder. On an empty space of the grid: Paste (when something has been copied), Select all; New folder…; Parent folder, Refresh; Find duplicates… (inside the Trash also Restore all and Empty the trash).

In the viewer, after the last image EyeCanSee stops; with the option **In the viewer, after the last image start again from the first** it goes back to the first one, and the other way round. EyeCanSee loads the images around the current one in advance, so moving between them is immediate even on slow connections.

## Supported formats

- **Photos**: JPEG, PNG, WebP, HEIC/HEIF, AVIF, JPEG XL, TIFF, GIF (first frame), BMP.
- **Camera RAW**: ARW, CR2, CR3, CRW, DNG, NEF, NRW, ORF, PEF, RAF, RW2, SRW, 3FR, IIQ, ERF, KDC, MEF, MOS, MRW and others. EyeCanSee shows the full-size preview stored by the camera; if there is none, it develops the sensor data. The DNG files of Samsung Galaxy phones cannot be developed yet (they would come out white): EyeCanSee shows the full-size preview the phone stores in them.
- **Vector**: SVG, SVGZ.
- **Graphics and HDR**: PSD (merged image), ORA, OpenEXR, HDR, DDS, ICNS, QOI, TGA, ICO, CUR, PCX, JPEG 2000, PNM, XPM, XBM, SGI, WBMP, OTB, MPO, JPS.
- **Through ImageMagick**: PSB, XCF, PFM, Sun Raster, PICT, MIFF, MNG, JNG, DPX, Cineon, FITS and other less common formats (XWD on Linux only). ImageMagick also opens the PSD files the built-in reader does not handle (for example 32-bit ones).

Photos are rotated automatically using their EXIF orientation. The file extension is only a hint: an iPhone HEIC photo renamed to `.jpg` opens anyway.

## Language

EyeCanSee uses the system language when it is available, otherwise English. To force a language set `EYECANSEE_LANG`, for example `EYECANSEE_LANG=en eyecansee`.

## Known limits

- On **NTFS** disks (for example an external disk also used with Windows) the Files app (Nautilus) cannot restore from the trash, not even files it deleted itself; EyeCanSee reminds you in the confirmation. The files are in the trash all the same and can be restored from a terminal: `gio trash --list` shows what is there, `gio trash --restore "<trash:/// address from that line>"` puts it back.
- If the phone does not answer a request (seen on Windows: one deletion that took over five minutes, with everything else waiting behind it), the status bar says so with the seconds elapsed and the file: EyeCanSee carries on by itself as soon as the phone answers. The cause is not known: it happened twice on a POCO X7 Pro, deleting files just created, and does not repeat on demand.
- **Windows**: right after a photo is taken, or a file deleted, on the phone, Windows' own driver can stop talking to the phone for about six minutes; unplugging the cable and plugging it in again frees it at once. More in [Windows stops talking to the phone](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.en.md).
- A phone opened through the file manager's connection (see External devices) is slower than the direct one: in folders with hundreds of files about one thumbnail per second.
- Images larger than 8192 pixels per side are shown downscaled.
- Files are sorted alphabetically: `img10` comes before `img2`.
- Hidden files and folders (starting with a dot, or on Windows with the "hidden" attribute) are
  not shown.
- HDR images (EXR, HDR, PFM) are shown without tone mapping and may look overexposed.
