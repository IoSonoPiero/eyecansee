# Image formats

What EyeCanSee opens and what it can save, on Linux (Fedora package and AppImage) and on Windows (installer and zip).

## Opening

| Format | Extensions | Linux | Windows | Decoder |
|---|---|:-:|:-:|---|
| JPEG | jpg, jpeg, jpe, jfif | ✓ | ✓ | libjpeg-turbo, `image` |
| MPO, JPS (multi-picture, stereo: the first picture) | mpo, jps | ✓ | ✓ | `image` |
| PNG | png | ✓ | ✓ | `image` |
| WebP | webp | ✓ | ✓ | `image` |
| GIF (first frame) | gif | ✓ | ✓ | `image` |
| BMP | bmp | ✓ | ✓ | `image` |
| TIFF | tif, tiff | ✓ | ✓ | `image` |
| ICO, CUR | ico, cur | ✓ | ✓ | `image` |
| TGA | tga | ✓ | ✓ | `image` |
| QOI | qoi | ✓ | ✓ | `image` |
| PNM | pbm, pgm, ppm, pnm, pam | ✓ | ✓ | `image` |
| OpenEXR | exr | ✓ | ✓ | `image` |
| Radiance HDR | hdr | ✓ | ✓ | `image` |
| DDS | dds | ✓ | ✓ | `image` |
| Farbfeld | ff | ✓ | ✓ | `image` |
| HEIC, HEIF | heic, heif, hif | ✓ | ✓ | libheif with libde265 |
| AVIF | avif | ✓ | ✓ | libheif with dav1d |
| JPEG XL | jxl | ✓ | ✓ | `jxl-oxide` |
| SVG | svg, svgz | ✓ | ✓ | `resvg` |
| Photoshop | psd | ✓ | ✓ | `psd` |
| JPEG 2000 | jp2, j2k, j2c, jpf, jpx | ✓ | ✓ | `hayro-jpeg2000` |
| OpenRaster | ora | ✓ | ✓ | `image-extras` |
| PCX | pcx | ✓ | ✓ | `image-extras` |
| SGI | sgi, rgb, rgba, bw | ✓ | ✓ | `image-extras` |
| WBMP, XBM, XPM, OTB | wbmp, xbm, xpm, otb | ✓ | ✓ | `image-extras` |
| Apple icon | icns | ✓ | ✓ | `image-extras` |
| Camera RAW (not yet tested with real camera files) | cr2, cr3, nef, arw, dng, raf, orf, rw2 and others | ✓ | ✓ | `rawler` |
| Photoshop large document | psb | ✓ | – | ImageMagick |
| GIMP | xcf | ✓ | – | ImageMagick |
| Sun raster | ras, sun | ✓ | – | ImageMagick |
| PICT | pict, pct | ✓ | – | ImageMagick |
| MIFF | miff | ✓ | – | ImageMagick |
| MNG, JNG | mng, jng | ✓ | – | ImageMagick |
| DPX, Cineon | dpx, cin | ✓ | – | ImageMagick |
| FITS | fits | ✓ | – | ImageMagick |
| XWD | xwd | ✓ | – | ImageMagick |
| PFM | pfm | ✓ | – | ImageMagick |
| PGX, WPG, VIFF, VICAR | pgx, wpg, viff, vicar | ✓ | – | ImageMagick |

A file with the wrong extension (for example an iPhone HEIC saved as `.jpg`) is recognised from its content. On Linux, a file the dedicated decoder cannot read is tried once more with ImageMagick.

## Saving (right click > Save in another format…)

The same formats on Linux and Windows: all the encoders are part of the program.

| Format | Extension | Options | Lossless | Keeps EXIF | Keeps colour profile | Encoder |
|---|---|---|:-:|:-:|:-:|---|
| JPEG | jpg | quality 1–100; colour detail 4:4:4, 4:2:2, 4:2:0 or greys only; progressive; optimised Huffman tables; arithmetic coding; lossless JPEG | lossless mode | ✓ | ✓ | libjpeg-turbo |
| PNG | png | compression level 0–9; filter (adaptive, none, Sub, Up, Average, Paeth) | ✓ | ✓ | ✓ | `image` |
| WebP | webp | lossy (quality 1–100, sharper colour edges) or lossless (near-lossless 0–100); effort 0–6 | lossless mode | ✓ | ✓ | libwebp |
| AVIF | avif | quality 1–100; transparency quality 1–100; speed 1–10; 8 or 10 bits per channel; RGB or YCbCr inside the file | – | ✓ | – | ravif (rav1e) |
| TIFF | tif | compression: none, LZW, Deflate (fast, normal, maximum), PackBits; horizontal predictor | ✓ | – | ✓ | `tiff` |
| BMP | bmp | – | ✓ | – | – | `image` |
| GIF | gif | colour reduction quality 1–30 (at most 256 colours) | – | – | – | `image` |
| QOI | qoi | – | ✓ | – | – | `image` |

- JPEG has no transparency: transparent parts are laid on white. Arithmetic coding and lossless JPEG make smaller or exact files that many programs cannot open.
- Metadata are carried over only from originals whose decoder reads them: EXIF from JPEG, PNG and WebP, the colour profile from JPEG, PNG, WebP, TIFF and GIF. Other originals (HEIC, AVIF, JPEG XL, RAW…) are saved without them for now.
- The EXIF orientation is reset to "upright", since the saved pixels are already turned.
- Not saved: HEIC (patent licensing of HEVC) and JPEG XL (no encoder in the program yet).
