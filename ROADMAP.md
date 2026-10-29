# Roadmap

The order is indicative: features arrive when they are ready and tested on real devices. Open an issue to ask for one, or vote for an existing one with a reaction.

## Next (before wider announcement)

- "Open with" for videos (Windows installer, `.desktop`, metainfo).
- Volume control in the video viewer.

## Video

- Save the current frame of a paused video.
- Full screen.
- 4K/60 drone videos.
- Media component tested on a KDE live system.

## Phones and servers

- Saved servers in Options, with passwords in the system keyring.
- Save to FTP (RNFR/RNTO rename).
- Embedded HEIC thumbnails.
- Xiaomi depth data preserved in rotations.
- Immich upload.
- Windows: a delete on the phone, or the call right after it, can stall for 5-6 minutes inside WPD and then recover (mtp-rs #52, under investigation upstream). The status bar now says how long the phone has not answered.
- Status bar in a phone's Android trash: show the file's own name, not the hidden `.trashed-…` one.
- Changes made on the phone itself while its folder is open: today the app finds out only when a read fails (the folder is then read again and the status bar names the file removed). mtp-rs 0.36.0 exposes the MTP events `ObjectAdded`/`ObjectRemoved` (`MtpDevice::next_event`, USB interrupt endpoint and WPD callbacks): listening to them would show the change at once.
- A folder listing on the phone that took 40 s once without a trace in the log: find out where the time goes if it happens again.

## Formats

- JPEG XL writing and more save formats (ICO, TGA, PNM, EXR, HDR, Farbfeld).
- More real Camera RAW samples from other brands, and real HEIC on Linux.
- JNG on Windows: the ImageMagick security policy shipped next to the program blocks it; relax it for the coders EyeCanSee needs.
- Lens correction for RAW with lensfun-rs.
- PDF import and export.

## Browsing and editing

- Favourites.
- Drag and drop to copy and move.
- Compressed archives.
- Multiple rename.
- Metadata editor.
- Similar images.
- Hide folders without images or videos.
- Set as wallpaper.
- Edits in a sidecar file.
- CPU/GPU/NPU acceleration with a "Check" button.

## Project

- Illustrated manual.
- Website.
- Windows code signing, when the project has enough history for SignPath or similar.
- Contributions upstream to magick-rust.
- Flathub: not possible under the current Flathub rules on AI-assisted code; to be reviewed if the rules change.
