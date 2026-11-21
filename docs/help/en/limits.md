# Known limits

What EyeCanSee does not do yet, or does only in part. The up-to-date list of known problems is in the "Known limitations" section of the [roadmap](https://github.com/IoSonoPiero/eyecansee/blob/main/ROADMAP.md).

## Images

- Images larger than 8192 pixels per side are shown reduced; the status bar still says their own size. SVG drawings are drawn at 2048 pixels on their long side.
- Pictures larger than 512 megapixels (width × height) do not open: the viewer says their size. A real photo of 300 megapixels opens.
- A picture that needs more than half of the computer's memory gets no thumbnail; the viewer, Save as and the editor open it only with **Open anyway**, a choice that holds until EyeCanSee is closed (see [Viewing images](viewer.md)), and Convert and Resize leave it out, saying so in their summary. Saving a turn over the original (**Save**, see [Rotating and flipping](rotate.md)) asks the same, or on the grid leaves it out; a JPEG turned without losing quality is not decoded, and never asked about.
- HDR images (EXR, HDR, PFM) are shown without tone mapping and may look overexposed.
- AVIF sequences show only their first frame in the viewer (animated GIF, PNG and WebP are played).
- Of a Photoshop file the viewer shows only the merged image, not its layers ([#122](https://github.com/IoSonoPiero/eyecansee/issues/122)).
- Ultra HDR photos are shown as normal photos, also on a screen that supports HDR ([#124](https://github.com/IoSonoPiero/eyecansee/issues/124)).
- EyeCanSee cannot save HEIC or JPEG XL files: see [Supported formats](formats.md).
- Converted Camera RAW photos do not keep the maker note, the camera maker's own data.
- On Windows, JNG files with transparency do not open: a check inside ImageMagick stops them.
- Sigma X3F files (Foveon sensors) do not open.
- Phase One P40+ files open at full size only where ImageMagick can read them (the Fedora package); in the AppImage and the Flatpak they show their small preview.
- Sony DSC-F828 files (SRF) open with a strong cyan cast: the camera has a four-colour sensor.

## Browsing

- Files are sorted alphabetically: `img10` comes before `img2`.
- Hidden files and folders (whose name starts with a dot, or on Windows with the "hidden" attribute) are not shown.
- A file or folder whose name is not valid UTF-8 (on Linux, for example one made by an old program or copied from an old disk) is shown with � in place of the characters that cannot be read, but it cannot be opened, copied or converted: rename it in the file manager.
- Changes made on a server by other programs are seen only after **F5**.

## Phones

- On Windows, right after a photo is taken, or a file is deleted, on the phone, Windows' own driver can stop talking to the phone for about six minutes. Unplugging the cable and plugging it in again frees it at once. More in [Windows stops talking to the phone](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.en.md).
- On Windows, if the connection to the phone breaks while Windows still sees the phone, after **Try again** the open folder keeps saying the phone was disconnected until EyeCanSee is started again. The file is saved anyway.
- Sometimes a phone does not answer a request for a long time (seen on Windows, deleting files just created). The status bar says so, with the seconds and the file, and EyeCanSee carries on by itself as soon as the phone answers.
- A phone opened through the file manager's connection is slower than the direct one: in folders with hundreds of files about one thumbnail per second.
- Photos deleted from the Xiaomi Gallery go to the Gallery's own private trash, which EyeCanSee cannot see.

## Disks and servers

- On Linux, on **NTFS** disks (for example an external disk also used with Windows) the Files app cannot restore from the trash, not even files it deleted itself. The files are in the trash all the same and can be restored from a terminal: `gio trash --list` shows what is there, `gio trash --restore "<trash:/// address from that line>"` puts it back.
- On Windows, network drives and CDs or DVDs do not appear in the panel: open them with **File > Open location…**.
- Copying from a phone or a server: an item whose name no file can have here (a name with `/`, `\`, `..`, a drive such as `C:` or control characters, which would lead out of the folder pasted) is left out, and the copy ends with a message that names it (the first ten, then how many more; ![Copy the message](icon://copy) copies every name). A moved folder that holds one stays where it was. On Windows the same happens to a name Windows cannot write, which a phone or a server allows: one with `:` or one of `< > " | ? *`, one ending in a dot or a space, or a name Windows keeps for its devices, such as `CON`, `NUL` or `COM1`, also with an extension (`NUL.jpg`).
- FTPS: a server that wants the data connection to resume the TLS session of the control connection may never send the data.
- SFTP: ProxyJump and ProxyCommand of `~/.ssh/config` are not supported.
- Pasting onto an FTP server that does not tell capitals apart (a Windows server, for example): a name that differs only in capitals from one there ("IMG_001.JPG" and "img_001.jpg") is not taken for the same name, so EyeCanSee does not ask, and the file there is written over. FTP shows names only as they are.
- Copying a folder from a server: on FTP a link to a folder is left out, as FTP does not say where it leads; on SFTP it is copied only when it leads outside the folders already copied, never to the folder itself, to a folder above or to one inside. In a move a link left out stays where it was, with the folders that hold it. At the end of the copy a message says how many links were left out, and names them (see [Copying and moving](copy-move.md)).

## 3D models and CAD drawings

- **3D models**: one light and no surroundings are reflected, so metals look less shiny than in 3D programs. In OBJ, Collada, 3DS, 3MF, VRML and X3D only the base colour and its picture are used. glTF 1.0 and VRML 1.0 files are not opened. Animations are not played: models show in their rest pose. Texts in VRML and X3D use a system font. A compressed model (3MF, zipped AMF and Collada, `.wrz`, `.x3dz`, compressed glTF) that unpacks to more than 100 times its size (past its first 16 MB), or to more than 2 GB in all, does not open: real models pack 5 to 10 times.
- **CAD drawings** are seen from above: 3D solids are not drawn, viewports show the model from above and cut to their rectangle, hatches too dense to see show as a light fill, a line type too fine at the current zoom is drawn continuous, texts whose SHX font is not found use a system font, and a picture attached to the drawing whose file is not next to it shows as a frame with a cross.

## Windows

- If EyeCanSee closes by itself at every start, before its window is shown, the graphics card's Vulkan driver may be what stops it (seen with an Intel driver). EyeCanSee draws with DirectX 12 unless Vulkan is chosen in **Options > Look**, and from version 0.78 a start with Vulkan that stops before the window is shown makes the next start go back to DirectX 12 by itself (see [Options](options.md)). With an older version, or if it still does not start, any one of these: set the environment variable `WGPU_BACKEND` to `dx12` (search for "environment variables" in the Start menu, add it to your account's variables) and start EyeCanSee again; remove the line `"graphics": "vulkan"` from `settings.json` (`%APPDATA%\eyecansee\settings.json` for the zip and the installer); for the copy from the Microsoft Store, **Settings > Apps > Installed apps > EyeCanSee > Advanced options > Reset**, which also clears all of EyeCanSee's settings and thumbnails.
- If the graphics card's driver closes EyeCanSee and the card cannot draw with DirectX 12 either (from version 0.78), EyeCanSee draws without the graphics card, with Windows' own software renderer: everything works, but more slowly, above all large pictures, videos and 3D models. At every start it says so. Update the driver from the website of the computer's maker (for a laptop or a ready-made computer) or of the graphics card's maker (Intel, AMD, NVIDIA), then choose **Vulkan** in **Options > Look** to try the card again; if after an update the card draws with DirectX 12, EyeCanSee goes back to it by itself. If the window cannot be opened at all, a message of Windows says to update the driver.
