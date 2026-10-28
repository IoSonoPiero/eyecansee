# Roadmap

The order is indicative: features arrive when they are ready and tested on real devices. Open an issue to ask for one, or vote for an existing one with a reaction.

## Next (before wider announcement)

- Thumbnails no longer retried while FFmpeg writes in the open folder.
- Minimum FFmpeg and libmpv versions checked, with a choice in Options.
- "Open with" for videos (Windows installer, `.desktop`, metainfo).
- Volume control in the video viewer.
- Fixes for issues #1 (PSD panic) and #2 (EXR thumbnail cache).

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

## Formats

- JPEG XL writing and more save formats (ICO, TGA, PNM, EXR, HDR, Farbfeld).
- More real Camera RAW samples from other brands, and real HEIC on Linux.
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
