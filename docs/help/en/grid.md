# The thumbnail grid

The grid shows what is in the open folder: first the ![..](icon://color-up) **..** cell, which goes up one level (not at the top of a disk, phone or server), then the subfolders, then the files as thumbnails.

## What you see in the grid

- ![Folder](icon://color-folder) **Folders**, by name.
- **Pictures**, each in its own tile with its name under it.
- **Videos**, with a frame of the video and the ![Video](icon://color-video) sign in the corner. See [Playing videos](video.md).
- **Audio files**, with their cover and the ![Audio](icon://color-audio) sign, or a note when they have no cover.
- **3D models** and **CAD drawings**, with a picture of the model or the drawing. See [3D models](model-3d.md) and [CAD drawings](cad.md).

Rest the mouse on a file to see its details: the full name with its extension (the name under the thumbnail may be cut), the size, the file date and, once its metadata have been read, the size in pixels. On a folder you see its name.

A turning circle means the thumbnail is still being made. The ![Download](icon://color-download) sign means that a component must be downloaded first to show that file (see [Add-on components](components.md)). Hidden files and folders are not shown. An empty folder says "This folder contains no images or subfolders".

Files you have copied or cut carry the ![Copied](icon://color-copied) or ![Cut](icon://color-cut) sign until you paste them, and cut files look faded (see [Copying and moving](copy-move.md)). A folder where you deleted something on a phone or a server has a strip at the top with **Open the trash** (see [Deleting and the trash](delete.md)).

## Moving around

- Click an item to select it. Double-click it, or press **Enter**, to open it: a folder shows its content, a file opens in the [viewer](viewer.md).
- **Backspace**, ![Parent folder](icon://arrow-up) **Parent folder** in the toolbar, or a double click on the **..** cell go up one level. The folder you came from is then selected.
- The **arrows** move the selection; **Home** and **End** go to the first and the last item. From the first item, the left arrow reaches the **..** cell.
- **Tab** goes to the next item; from the last one it moves on to the side panel. See [Folders and the side panel](folders.md).

When you enter a folder, nothing is selected: the first arrow you press selects the first item.

## Order of the files

The drop-down list next to the address bar chooses the order: **Name**, **Modification date**, **Size** or **Type** (the file extension). The button next to it turns the order round: ![Ascending order](icon://arrow-up-narrow-wide) **Ascending order** or ![Descending order](icon://arrow-down-wide-narrow) **Descending order**. The same choices are in **View > Sort by**.

Folders always come first, by name. Sorting by date, files without a date go to the end. EyeCanSee remembers your choice.

## Which files to show

**View > Show** (also in the right-click menu of an empty space of the grid) chooses what the grid shows:

- **Images, videos and audio** (at first): everything.
- **Images only**: pictures, 3D models and CAD drawings, without videos and audio.
- **Videos only** or **Audio only**.

The choice stays for every folder until you change it. The status bar counts images, videos and audio files apart.

## Size of the thumbnails

The slider at the right end of the toolbar, next to ![Thumbnail size](icon://image), makes the thumbnails larger or smaller. The cells spread over the whole width of the grid.

## Thumbnails

EyeCanSee makes the thumbnail of the selected file first, then those visible in the window, then the others of the folder. Thumbnails are saved on disk, so the next time the folder opens at once; a thumbnail is made again when its file changes.

If a thumbnail looks wrong, right-click the file (or several selected files) and choose **Redo thumbnails**. **View > Redo all thumbnails in the folder** (**Shift+F5**) makes them all again. The space the thumbnails take on disk is set in [Options](options.md).

## Refreshing

**F5**, or ![Refresh](icon://refresh-cw) **Refresh** in the toolbar, reads the folder again. A folder of this computer updates by itself when another program adds, deletes or renames files in it; on a server press **F5**.

## The right-click menu

- On a **file**: the actions that work on it, such as Open, Slideshow, Edit image…, Convert and resize…, Cut, Copy, Rename… and Delete…. With several files selected, the menu works on all of them (see [Selecting files](selection.md)).
- On a **folder**: Open, New folder inside…, Cut, Copy, Paste into this folder, Find duplicates…, Rename…, Delete…, Delete for good….
- On an **empty space**: Paste (when you have copied something), Select all, New folder…, Parent folder, Refresh, Show and Find duplicates….

An entry marked with **\*** works only on some of the files chosen (for example Convert and resize on the pictures and videos among them); a line at the bottom of the menu says so.

## The status bar

At the bottom of the window: how many folders, images, videos and audio files the folder has, and the name, size and date of the selected file (of a folder, its name). With several items selected it shows how many they are and how much their files weigh. After Copy or Cut it reminds you to press Ctrl+V.
