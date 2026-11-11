# Known limits

What EyeCanSee does not do yet, or does only in part. The up-to-date list of known problems is in the "Known limitations" section of the [roadmap](https://github.com/IoSonoPiero/eyecansee/blob/main/ROADMAP.md).

## Images

- Images larger than 8192 pixels per side are shown reduced.
- HDR images (EXR, HDR, PFM) are shown without tone mapping and may look overexposed.
- EyeCanSee cannot save HEIC or JPEG XL files: see [Supported formats](formats.md).
- Converted Camera RAW photos do not keep the maker note, the camera maker's own data.
- On Windows, JNG files do not open: the ImageMagick security policy that comes with the program blocks them.

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
- FTPS: a server that wants the data connection to resume the TLS session of the control connection may never send the data.
- SFTP: ProxyJump and ProxyCommand of `~/.ssh/config` are not supported.

## 3D models and CAD drawings

- **3D models**: one light and no surroundings are reflected, so metals look less shiny than in 3D programs. In OBJ, Collada, 3DS, 3MF, VRML and X3D only the base colour and its picture are used. glTF 1.0 and VRML 1.0 files are not opened. Animations are not played: models show in their rest pose. Texts in VRML and X3D use a system font.
- **CAD drawings** are seen from above: 3D solids are not drawn, viewports show the model from above and cut to their rectangle, hatches too dense to see show as a light fill, a line type too fine at the current zoom is drawn continuous, texts whose SHX font is not found use a system font, and a picture attached to the drawing whose file is not next to it shows as a frame with a cross.

## Look

- On Linux, **Colors > Same as the system** is always dark: EyeCanSee does not read the desktop's theme yet. Choose **Light** in **Options > Look** for light colors.
