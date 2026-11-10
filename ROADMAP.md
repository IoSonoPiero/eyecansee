# Roadmap

The order is indicative: features arrive when they are ready and tested on real devices. Open an issue to ask for one, or vote for an existing one with a reaction.

## Planned

### First

- Saving without the GPS position, or without metadata ([#55](https://github.com/IoSonoPiero/eyecansee/issues/55)).
- Image editor: an SVG stays an SVG ([#56](https://github.com/IoSonoPiero/eyecansee/issues/56)).

### Next

- Upload to Immich ([#13](https://github.com/IoSonoPiero/eyecansee/issues/13)).
- Lens correction for Camera RAW photos (lensfun) ([#14](https://github.com/IoSonoPiero/eyecansee/issues/14)).
- Drag and drop to copy and move ([#15](https://github.com/IoSonoPiero/eyecansee/issues/15)).

### Video

- Save the frame of a paused video as an image ([#16](https://github.com/IoSonoPiero/eyecansee/issues/16)).
- Full screen: hide the video controls too until the mouse moves ([#43](https://github.com/IoSonoPiero/eyecansee/issues/43)).
- Smoother playback of 4K/60 videos ([#17](https://github.com/IoSonoPiero/eyecansee/issues/17)).

### Devices and servers

- Options per saved server in the server manager (FTP active mode, text encoding, ciphers) when a server needs them ([#18](https://github.com/IoSonoPiero/eyecansee/issues/18)).
- Encrypting files and folders with a password ([#19](https://github.com/IoSonoPiero/eyecansee/issues/19)).
- Faster HEIC thumbnails from the preview stored in the file ([#20](https://github.com/IoSonoPiero/eyecansee/issues/20)).
- Windows: phone photos opened from File Explorer go through the direct connection, so they can be saved back to the phone ([#52](https://github.com/IoSonoPiero/eyecansee/issues/52)).

### Formats

- Save in JPEG XL ([#21](https://github.com/IoSonoPiero/eyecansee/issues/21)).
- Camera RAW photos from more brands ([#7](https://github.com/IoSonoPiero/eyecansee/issues/7)).
- PDF: images to a PDF, and a PDF to images ([#22](https://github.com/IoSonoPiero/eyecansee/issues/22)).
- 3D models and CAD files: materials beyond the base colour and its picture ([#23](https://github.com/IoSonoPiero/eyecansee/issues/23)).
- Read the .xmp sidecar files next to RAW photos ([#24](https://github.com/IoSonoPiero/eyecansee/issues/24)).

### Browsing and editing

- Image editor: light and colour corrections (exposure, saturation, automatic levels, colour temperature, invert) ([#60](https://github.com/IoSonoPiero/eyecansee/issues/60)).
- Open an image from the clipboard ([#57](https://github.com/IoSonoPiero/eyecansee/issues/57)).
- Image editor: the preview where a filter and a highlighter overlap ([#58](https://github.com/IoSonoPiero/eyecansee/issues/58)).
- Image editor: recent work kept, recovered after a crash, resumed later ([#59](https://github.com/IoSonoPiero/eyecansee/issues/59)).
- Archives: zip and 7z opened as read-only folders first, then creating zip archives ([#26](https://github.com/IoSonoPiero/eyecansee/issues/26)).
- Multiple rename ([#27](https://github.com/IoSonoPiero/eyecansee/issues/27)).
- Metadata editor for EXIF and GPS ([#28](https://github.com/IoSonoPiero/eyecansee/issues/28)).
- Similar images (the same photo resized or recompressed) ([#29](https://github.com/IoSonoPiero/eyecansee/issues/29)).
- Hide folders without images or videos ([#30](https://github.com/IoSonoPiero/eyecansee/issues/30)).
- Deleting a folder: look into its subfolders too for files EyeCanSee does not show (today the warning checks the first level only) ([#31](https://github.com/IoSonoPiero/eyecansee/issues/31)).
- Set as wallpaper ([#32](https://github.com/IoSonoPiero/eyecansee/issues/32)).
- Options: download the add-on engines (video engine, FFmpeg, CAD engine) in advance, and remove them ([#50](https://github.com/IoSonoPiero/eyecansee/issues/50)).

## Later, if requested

- Immich as a source to browse, like a server or a phone ([#35](https://github.com/IoSonoPiero/eyecansee/issues/35)).

## Known limitations

- Windows: after a photo is taken or a file is deleted on the phone, the Windows MTP driver can stop answering for a few minutes. See [docs/phone-stall.en.md](docs/phone-stall.en.md) ([#36](https://github.com/IoSonoPiero/eyecansee/issues/36)).
- FTPS: a server that wants the data connection to resume the TLS session of the control connection may never send the data ([#37](https://github.com/IoSonoPiero/eyecansee/issues/37)).
- Windows: JNG files do not open, because the ImageMagick security policy shipped with the program blocks them ([#38](https://github.com/IoSonoPiero/eyecansee/issues/38)).
- Xiaomi phones: photos deleted from the Xiaomi Gallery go to the Gallery's own private trash, which EyeCanSee cannot see ([#39](https://github.com/IoSonoPiero/eyecansee/issues/39)).
- Windows: if the connection to the phone breaks while Windows still sees the phone, after Try again the open folder keeps saying the phone was disconnected until EyeCanSee is restarted. The file is saved anyway ([#40](https://github.com/IoSonoPiero/eyecansee/issues/40)).
- Converted Camera RAW photos do not keep the maker note, the camera maker's own data ([#41](https://github.com/IoSonoPiero/eyecansee/issues/41)).
- Windows: a file owned by another account (for example created as administrator) and replaced with Paste > Replace gets the current user as owner. Its content is right; files owned by the user are not affected ([#93](https://github.com/IoSonoPiero/eyecansee/issues/93)).

## Not planned

- AI upscaling (Real-ESRGAN and similar): the models are large and their licenses vary. It could come back as an optional download if there is demand.
- Flathub: the Flatpak comes from EyeCanSee's own repository, signed by the developer (https://flatpak.montinaro.info). Publishing it on Flathub may be considered later.
