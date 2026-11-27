# Roadmap

The order is indicative: features arrive when they are ready and tested on real devices. Open an issue to ask for one, or vote for an existing one with a reaction.

## Planned

### First

- Image editor: an SVG stays an SVG ([#56](https://github.com/IoSonoPiero/eyecansee/issues/56)).

### Next

- Encrypting and decrypting files and folders with a password, in the age format that opens also without EyeCanSee ([#19](https://github.com/IoSonoPiero/eyecansee/issues/19)).
- Upload to Immich ([#13](https://github.com/IoSonoPiero/eyecansee/issues/13)).
- Lens correction for Camera RAW photos (lensfun) ([#14](https://github.com/IoSonoPiero/eyecansee/issues/14)).
- Drag and drop to copy and move ([#15](https://github.com/IoSonoPiero/eyecansee/issues/15)).

### Video

- Smoother playback of 4K/60 videos ([#17](https://github.com/IoSonoPiero/eyecansee/issues/17)).
- Convert: keep the FFmpeg encoder check between starts instead of running it again each time ([#112](https://github.com/IoSonoPiero/eyecansee/issues/112)).
- Windows: the Convert window narrow the first time, while FFmpeg checks the encoders ([#136](https://github.com/IoSonoPiero/eyecansee/issues/136)).
- Crop a video and rotate it by any angle, as a photo ([#129](https://github.com/IoSonoPiero/eyecansee/issues/129)).
- Video: frame by frame with the keys and a finer timeline, to reach a precise moment ([#165](https://github.com/IoSonoPiero/eyecansee/issues/165)).
- Video: subtitles, with a choice of the track ([#193](https://github.com/IoSonoPiero/eyecansee/issues/193)).
- 360° videos played as a sphere to look around in ([#308](https://github.com/IoSonoPiero/eyecansee/issues/308)).

### Devices and servers

- A picture open from a phone or a server kept on this computer, so that saving does not download it again ([#313](https://github.com/IoSonoPiero/eyecansee/issues/313)).
- Options per saved server in the server manager (FTP active mode, text encoding, ciphers) when a server needs them ([#18](https://github.com/IoSonoPiero/eyecansee/issues/18)).
- Faster HEIC thumbnails from the preview stored in the file ([#20](https://github.com/IoSonoPiero/eyecansee/issues/20)).
- Windows: phone photos opened from File Explorer go through the direct connection, so they can be saved back to the phone ([#52](https://github.com/IoSonoPiero/eyecansee/issues/52)).
- Side panel: the folders of the open server, down to the one open, as for This computer ([#110](https://github.com/IoSonoPiero/eyecansee/issues/110)).
- Saved servers: log in through the SSH agent ([#133](https://github.com/IoSonoPiero/eyecansee/issues/133)).
- Merging a folder with many replaced files on phones and SFTP servers as fast as before the safe replace of 0.76.0 ([#94](https://github.com/IoSonoPiero/eyecansee/issues/94)).
- A disk, USB stick or phone unplugged while its folder is open: the status bar says so ([#281](https://github.com/IoSonoPiero/eyecansee/issues/281)).
- The left panel shows the folder open on a server, a phone or a device under its name, as it does for This computer ([#314](https://github.com/IoSonoPiero/eyecansee/issues/314)).
- A file of a server or a phone opened again comes from a copy on this computer when it has not changed; the space for the copies chosen in the Options ([#357](https://github.com/IoSonoPiero/eyecansee/issues/357)).

### Formats

- Save in JPEG XL ([#21](https://github.com/IoSonoPiero/eyecansee/issues/21)).
- PDF: images to a PDF, and a PDF to images ([#22](https://github.com/IoSonoPiero/eyecansee/issues/22)).
- 3D models and CAD drawings: materials beyond the base colour and its texture ([#23](https://github.com/IoSonoPiero/eyecansee/issues/23)).
- Viewer: show HDR photos in HDR on screens that support it ([#124](https://github.com/IoSonoPiero/eyecansee/issues/124)).
- Viewer: play animated AVIF ([#131](https://github.com/IoSonoPiero/eyecansee/issues/131)).
- Viewer: 360° panoramas shown as a sphere to look around in ([#188](https://github.com/IoSonoPiero/eyecansee/issues/188)).
- Blender .blend files in the grid and the viewer, with the preview saved in the file ([#352](https://github.com/IoSonoPiero/eyecansee/issues/352)).

### CAD drawings and 3D models

- CAD drawings: export the sheets (layouts) to PDF, to scale ([#140](https://github.com/IoSonoPiero/eyecansee/issues/140)).
- STEP, IGES and BREP models: a file too heavy for the CAD engine stops after a time limit, without blocking the other files or closing EyeCanSee ([#175](https://github.com/IoSonoPiero/eyecansee/issues/175)).
- 3D models keep the names, groups, hierarchy and transparency of their parts when opened and exported ([#347](https://github.com/IoSonoPiero/eyecansee/issues/347)).

### Browsing and editing

- Viewer: Esc closes EyeCanSee when the window's ✕ is set to close it ([#320](https://github.com/IoSonoPiero/eyecansee/issues/320)).
- Viewer: a double click can go to full screen instead of back to the grid, chosen in the Options ([#349](https://github.com/IoSonoPiero/eyecansee/issues/349)).
- Viewer: a Full screen entry in the View menu and in the viewer's right-click menu ([#358](https://github.com/IoSonoPiero/eyecansee/issues/358)).
- Grid: type the first letters of a name to jump to that file or folder ([#350](https://github.com/IoSonoPiero/eyecansee/issues/350)).
- Bookmarks in the left panel, as well as in the menu and the toolbar ([#351](https://github.com/IoSonoPiero/eyecansee/issues/351)).
- Menus: the menu bar opened from the keyboard (Alt, F10, Alt + letter), as in other desktop apps ([#322](https://github.com/IoSonoPiero/eyecansee/issues/322)).
- Paste: what was pasted is selected in the folder, the last item when there are several ([#323](https://github.com/IoSonoPiero/eyecansee/issues/323)).
- F6 takes the keyboard to Cancel of a copy or a conversion under way ([#324](https://github.com/IoSonoPiero/eyecansee/issues/324)).
- Windows: EyeCanSee in Default apps with all its formats, a button to open that page, and "Open with EyeCanSee" in the right-click menu ([#325](https://github.com/IoSonoPiero/eyecansee/issues/325)).
- Image editor: light and colour corrections (exposure, saturation, automatic levels, colour temperature, invert) ([#60](https://github.com/IoSonoPiero/eyecansee/issues/60)).
- Image editor: magic wand and fill by colour ([#146](https://github.com/IoSonoPiero/eyecansee/issues/146)).
- Image editor: clone stamp ([#147](https://github.com/IoSonoPiero/eyecansee/issues/147)).
- Image editor: recolour ([#148](https://github.com/IoSonoPiero/eyecansee/issues/148)).
- Image editor: gradient ([#149](https://github.com/IoSonoPiero/eyecansee/issues/149)).
- Prepare for sharing: resize, convert, remove metadata and stay under a file size, in one step ([#115](https://github.com/IoSonoPiero/eyecansee/issues/115)).
- Output profiles: saved settings applied the same way to a series of images ([#116](https://github.com/IoSonoPiero/eyecansee/issues/116)).
- New window: more than one EyeCanSee window at the same time ([#130](https://github.com/IoSonoPiero/eyecansee/issues/130)).
- Image editor: recent work kept, recovered after a crash, resumed later ([#59](https://github.com/IoSonoPiero/eyecansee/issues/59)).
- Image editor: a working file (.eye) that keeps texts, arrows and pasted pieces editable ([#138](https://github.com/IoSonoPiero/eyecansee/issues/138)).
- Archives: zip and 7z opened as read-only folders first, then creating zip archives ([#26](https://github.com/IoSonoPiero/eyecansee/issues/26)).
- Thumbnails: invert the selection ([#135](https://github.com/IoSonoPiero/eyecansee/issues/135)).
- Rename several files at once ([#27](https://github.com/IoSonoPiero/eyecansee/issues/27)).
- Metadata editor for EXIF and GPS ([#28](https://github.com/IoSonoPiero/eyecansee/issues/28)).
- Similar images (the same photo resized or recompressed) ([#29](https://github.com/IoSonoPiero/eyecansee/issues/29)).
- Hide folders without images or videos ([#30](https://github.com/IoSonoPiero/eyecansee/issues/30)).
- Deleting a folder: look into its subfolders too for files EyeCanSee does not show (today the warning checks the first level only) ([#31](https://github.com/IoSonoPiero/eyecansee/issues/31)).
- Set as wallpaper ([#32](https://github.com/IoSonoPiero/eyecansee/issues/32)).
- Copy files from the grid to other file managers ([#64](https://github.com/IoSonoPiero/eyecansee/issues/64)).
- Options: download the add-on engines (video engine, FFmpeg, CAD engine) in advance, and remove them ([#50](https://github.com/IoSonoPiero/eyecansee/issues/50)).
- A window at the first start to choose the add-on engines to download (and, on Windows, the desktop icon) ([#212](https://github.com/IoSonoPiero/eyecansee/issues/212)).
- Windows: a desktop icon for the Microsoft Store and zip copies too ([#210](https://github.com/IoSonoPiero/eyecansee/issues/210)).
- Window at startup: EyeCanSee reopens at the size it had when closed, by default ([#221](https://github.com/IoSonoPiero/eyecansee/issues/221)).
- Options: a switch to write a diagnostic log, to attach to a problem report ([#189](https://github.com/IoSonoPiero/eyecansee/issues/189)).
- Slideshow: a key to start it ([#208](https://github.com/IoSonoPiero/eyecansee/issues/208)).
- Names sorted the natural way: img2 before img10 ([#272](https://github.com/IoSonoPiero/eyecansee/issues/272)).
- Image editor: select several objects at once ([#277](https://github.com/IoSonoPiero/eyecansee/issues/277)).
- Image editor: lock an object, so that a click reaches what is under it ([#278](https://github.com/IoSonoPiero/eyecansee/issues/278)).

## Later, if requested

- Immich as a source to browse, like a server or a phone ([#35](https://github.com/IoSonoPiero/eyecansee/issues/35)).

## Known limitations

- Windows: after a photo is taken or a file is deleted on the phone, the Windows MTP driver can stop answering for a few minutes. See [docs/phone-stall.en.md](docs/phone-stall.en.md) ([#36](https://github.com/IoSonoPiero/eyecansee/issues/36)).
- FTPS: a server that wants the data connection to resume the TLS session of the control connection may never send the data ([#37](https://github.com/IoSonoPiero/eyecansee/issues/37)).
- Windows: JNG files with transparency do not open, stopped by a check inside ImageMagick ([#38](https://github.com/IoSonoPiero/eyecansee/issues/38)).
- Xiaomi phones: photos deleted from the Xiaomi Gallery go to the Gallery's own private trash, which EyeCanSee cannot see ([#39](https://github.com/IoSonoPiero/eyecansee/issues/39)).
- Windows: if the connection to the phone breaks while Windows still sees the phone, after Try again the open folder keeps saying the phone was disconnected until EyeCanSee is restarted. The file is saved anyway ([#40](https://github.com/IoSonoPiero/eyecansee/issues/40)).
- Converted Camera RAW photos do not keep the maker note, the camera maker's own data ([#41](https://github.com/IoSonoPiero/eyecansee/issues/41)).
- GIMP files (XCF) keep only their colour profile when converted: GIMP stores EXIF, XMP and IPTC as text of its own, which ImageMagick does not read ([#42](https://github.com/IoSonoPiero/eyecansee/issues/42)).
- Windows: a file owned by another account (for example created as administrator) and replaced with Paste > Replace gets the current user as owner. Its content is right; files owned by the user are not affected ([#93](https://github.com/IoSonoPiero/eyecansee/issues/93)).
- Viewer: of an AVIF sequence only the first frame is shown; animated GIF, WebP and APNG images play ([#131](https://github.com/IoSonoPiero/eyecansee/issues/131)).
- Camera RAW: Sigma X3F files do not open, Sony SRF files open with a cyan cast, Phase One P40+ files show only their preview outside the Fedora package ([#7](https://github.com/IoSonoPiero/eyecansee/issues/7)).

## Not planned

- AI upscaling (Real-ESRGAN and similar): the models are large and their licenses vary. It could come back as an optional download if there is demand.
- Flathub: the Flatpak comes from EyeCanSee's own repository, signed by the developer (https://flatpak.montinaro.info). Publishing it on Flathub may be considered later.
