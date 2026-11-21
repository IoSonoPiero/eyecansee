# Viewing images

The viewer shows one picture at a time, as large as the window allows. From there you move to the next picture, zoom in, turn the picture, and go back to the grid.

![Viewing images](../img/viewer.webp)

## Opening and closing the viewer

- Double-click a picture in the grid, or select it and press **Enter** (also **View > Open**).
- To go back to the grid: **Esc**, a double click on the picture, ![Back to the grid](icon://layout-grid) **Back to the grid** in the toolbar, or **View > Back to the grid**. The picture you were looking at is then selected in the grid.
- The window's ✕ also goes back to the grid instead of closing EyeCanSee. You can change this in [Options](options.md).

With several files selected, Enter opens the first of them, and the viewer moves only among them.

## Moving between pictures

| Key | What it does |
|---|---|
| →, Space, Page Down | Next picture |
| ←, Backspace, Page Up | Previous picture |
| Home, End | First, last picture |

The same items are in the **View** menu (Next, Previous, First, Last). After the last picture the viewer stops; in [Options](options.md) you can make it start again from the first. The pictures around the one shown are read in advance, so moving between them is quick even on a phone or a server.

## Very large pictures

A picture larger than 8192 pixels on its long side is shown reduced to 8192 pixels; a JPEG is read already reduced, so it takes less memory. A picture that would need more than half of the computer's memory to open (for example a HEIC, a 16-bit TIFF, a Photoshop file or a JPEG XL of hundreds of megapixels, on a computer with 8 GB) is not opened at once: in its place the viewer says how much memory it needs and offers **Open anyway**. Opened anyway, the computer may become very slow, or the system may close EyeCanSee. Once opened anyway, that picture opens without asking (also in the editor, Save as, Convert and Resize) until EyeCanSee is closed; when EyeCanSee starts again, the viewer asks again. The arrows go on to the next picture as usual; a picture not opened stays closed, and the viewer says so again when you come back to it. A picture that large is never prepared in advance as a neighbour, even once opened anyway: its file is read whole at once when it is up to 32 MB, and of a larger one only the start, when the size of the picture is there (a TIFF often has it at the end, and is read whole), and it is not opened; the slideshow does not show it, unless it was opened anyway: then it opens when its turn comes. In the grid it has no thumbnail (see [The thumbnail grid](grid.md)).

## Animations and pages

Animated GIF, WebP and PNG (APNG) pictures play in the viewer, over and over. **Space** pauses and plays them again, as does the button in the small bar at the bottom of the picture, which also shows the frame (for example Frame 3 / 20, also in the status bar). The animation stops when you move to another picture and starts again from the first frame when you come back. A very long or very large animation (more than 256 MB once decoded) is not played: the viewer shows its first frame and says why.

A TIFF with several pages (a scanned document, a fax) opens on its first page. **Page Down** and **Page Up** turn the pages, as do the arrows in the bar at the bottom, which shows the page (for example Page 2 / 5, also in the status bar); past the last page Page Down goes to the next file, and before the first Page Up to the previous one. Each page is fitted to the window.

## Motion photos

A motion photo (Google Pixel, Samsung, Xiaomi and other phones) carries a short video recorded with the photo. The viewer shows the photo with a **Play motion** button at the bottom; the button or **Space** plays the video once, with the bar of a [video](video.md), then the photo comes back. The video needs the video engine, as all videos do.

## Zooming

- The **mouse wheel** zooms in and out around the pointer; **+** and **−** around the middle of the window. On a touchpad, pinch.
- **0** fits the picture to the window again; **1** shows it at 100%, one pixel of the picture for each pixel of the screen. The percentages count the pixels of the file, also for a very large photo shown reduced and for an SVG drawing (at the size it declares).
- At the right of the status bar, beside **Keys**: ![Zoom out](icon://zoom-out) and ![Zoom in](icon://zoom-in) do as **−** and **+**; click the percentage and type another (for example **37** and **Enter**; **Esc** leaves it as it was), or drag the slider beside it: its marks stand on round zooms (10%, 25%, 50%, 100%, 200%…), and the closer to the left, the smaller the picture.
- Drag a zoomed picture to move it.
- Below 100% each point of the screen shows the average of the pixels it covers, at every zoom: edges stay smooth and fine details (grass, hair) do not turn grainy. It can be turned off in [Options](options.md) > Look.

A picture opens fitted to the window, and a small picture at its real size, not enlarged. You can make it smaller than fitted, down to 1% (a long panorama stops when its short side is 16 pixels on the screen): zooming out, the wheel and **−** stop once at fitted, then go on; smaller than the window, the picture stays in the middle. An SVG drawing is enlarged to fit the window, also when it is smaller. A CAD drawing is never shown smaller than fitted. The largest zoom (2000% at first) is set in [Options](options.md); for an SVG drawing smaller than 2048 pixels it counts on the 2048 pixels it is drawn at, so its percentage can go much higher. In Options you can also make the wheel change picture instead of zooming: then **Ctrl** and the wheel still zoom. The next picture starts fitted again.

## The status bar of the viewer

At the bottom: the file name, its position in the folder (for example 12 / 51), its size in pixels (its own, as in the metadata panel, also when it is shown reduced, and for an SVG drawing the size it declares), its size on disk, and its date: the date taken when its metadata were already read in the grid, otherwise the date the file was modified. After a turn not yet saved it also says **Changed, not saved**. On the right, on a picture, the zoom with its buttons and slider (see Zooming, above).

The **Keys** button on the right lists the keys of what you are looking at (a picture, a video, a 3D model or a CAD drawing): while the mouse is on it, or until you click it again.

## The toolbar in the viewer

The toolbar keeps only what works on the picture shown: ![Rename…](icon://text-cursor-input) **Rename…**, ![Delete…](icon://trash) **Delete…**, ![Rotate 90° anticlockwise](icon://rotate-ccw-square) and ![Rotate 90° clockwise](icon://rotate-cw-square) to turn it, ![Edit image…](icon://square-pen) **Edit image…**, ![Save](icon://save) **Save** and ![Save as…](icon://file-output) **Save as…**, then the bookmarks and the address bar. New folder, cut, copy, paste and the tools are for the grid; they stay in the menus. On a video or an audio file the buttons for pictures stay in their place, greyed out: pointing at one says why.

## Full screen

**F11** shows the picture alone, without menus and bars, on black. **Esc** first leaves full screen, then goes back to the grid.

A video at full screen fills the screen: its bar appears over the bottom of the picture when the mouse moves, and hides with the pointer after 2 seconds with the mouse still (it stays while the pointer is on it).

## What else you can do

- Rotate and flip the picture: see [Rotating and flipping](rotate.md).
- Draw on it, crop it, hide parts of it: see [Editing an image](editor.md) (**Ctrl+E**).
- Save a copy in another format: see [Save as](save-as.md) (**Ctrl+Shift+S**).
- **F2** renames the picture and **Del** deletes it, as in the grid; after a deletion the next picture appears.
- A right click on the picture opens a menu with these actions and more.
