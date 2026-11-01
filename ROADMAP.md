# Roadmap

The order is indicative: features arrive when they are ready and tested on real devices. Open an issue to ask for one, or vote for an existing one with a reaction.

## Planned

### First

- Jump forward and back in a video with Ctrl and the arrow keys.
- Notification windows with the parts to compare highlighted (SFTP and FTPS fingerprints) and one common style.
- Enter and Esc close the result windows.

### Next

- Upload to Immich.
- Lens correction for Camera RAW photos (lensfun).
- Drag and drop to copy and move.

### Video

- Save the frame of a paused video as an image.
- Full screen.
- Rotated videos from phones shown the right way up.
- 4K/60 videos.

### Devices and servers

- Options per saved server in the server manager (FTP active mode, text encoding, ciphers) when a server needs them.
- Encrypting files and folders with a password.
- Faster HEIC thumbnails from the preview stored in the file.

### Formats

- Saving in JPEG XL, ICO, TGA, PNM, EXR, HDR and Farbfeld.
- Camera RAW photos from more brands.
- PDF: images to a PDF, and a PDF to images.

### Browsing and editing

- Favourites: a list of folders and images to open with one click.
- Archives: zip and 7z opened as read-only folders first, then creating zip archives.
- Multiple rename.
- Metadata editor for EXIF and GPS.
- Similar images (the same photo resized or recompressed).
- Hide folders without images or videos.
- Set as wallpaper.
- Edits kept in a sidecar file, with the original untouched.
- CPU/GPU/NPU acceleration with a "Check" button that enables what works on the PC.

## Documentation

- Illustrated manual.

## Later, if requested

- Immich as a source to browse, like a server or a phone.

## Known limitations

- Windows: after a photo is taken or a file is deleted on the phone, the Windows MTP driver can stop answering for a few minutes. See [docs/phone-stall.en.md](docs/phone-stall.en.md).
- FTPS: a server that wants the data connection to resume the TLS session of the control connection may never send the data.
- Windows: JNG files do not open, because the ImageMagick security policy shipped with the program blocks them.
- Xiaomi phones: photos deleted from the Xiaomi Gallery go to the Gallery's own private trash, which EyeCanSee cannot see.

## Not planned

- AI upscaling (Real-ESRGAN and similar): the models are large and their licenses vary. It could come back as an optional download if there is demand.
- Flatpak and Flathub: there is no Flatpak package, and EyeCanSee is not distributed on Flathub, because of the catalogue's current rules. On Linux it comes as an AppImage and as an RPM for Fedora.
