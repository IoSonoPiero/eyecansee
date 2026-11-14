# Supported formats

What EyeCanSee opens, and what it can save. Everything here works the same on Linux and on Windows, except where a line says otherwise. The containers, codecs and encoders for converting videos are listed in [video formats](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/video-formats.md).

## Opening

### Images

| Format | Extensions | Read with |
|---|---|---|
| JPEG | jpg, jpeg, jfif | libjpeg-turbo, `image` |
| MPO, JPS (multi-picture and stereo: the first picture) | mpo, jps | `image` |
| PNG (animated: the first frame) | png, apng | `image` |
| WebP | webp | `image` |
| GIF (the first frame) | gif | `image` |
| BMP | bmp | `image` |
| TIFF | tif, tiff | `image` |
| ICO, CUR | ico, cur | `image` |
| TGA | tga | `image` |
| QOI | qoi | `image` |
| PNM | pbm, pgm, ppm, pnm, pam | `image` |
| OpenEXR | exr | `image` |
| Radiance HDR | hdr | `image` |
| Farbfeld | ff | `image` |
| HEIC, HEIF | heic, heif, hif | libheif with libde265 |
| AVIF | avif | libheif with dav1d |
| JPEG XL | jxl | `jxl-oxide` |
| SVG | svg, svgz | `resvg` |
| Photoshop (the merged image) | psd | `psd` |
| JPEG 2000 | jp2, j2k, j2c, jpf, jpx | `hayro-jpeg2000` |
| DDS | dds | `image-extras` |
| OpenRaster | ora | `image-extras` |
| PCX | pcx | `image-extras` |
| SGI | sgi, rgb, rgba, bw | `image-extras` |
| WBMP, XBM, XPM, OTB | wbmp, xbm, xpm, otb | `image-extras` |
| Apple icon | icns | `image-extras` |
| Photoshop large document | psb | ImageMagick |
| GIMP | xcf | ImageMagick |
| Sun raster | ras, sun | ImageMagick |
| PICT | pict, pct | ImageMagick |
| MIFF | miff | ImageMagick |
| MNG, JNG | mng, jng | ImageMagick |
| DPX, Cineon | dpx, cin | ImageMagick |
| FITS | fits | ImageMagick |
| PFM | pfm | ImageMagick |
| PGX, WPG, VIFF, VICAR | pgx, wpg, viff, vicar | ImageMagick |
| XWD (only with the Fedora package, not in the AppImage, the Flatpak or on Windows) | xwd | ImageMagick |

Photos are turned upright using their EXIF orientation. The extension is only a hint: a file with the wrong one (for example an iPhone HEIC photo renamed to `.jpg`) is recognised from its content. A file the reader of its format cannot read is tried once more with ImageMagick, which also opens the PSD files the built-in reader does not handle.

### Camera RAW

3FR, ARI, ARW, CR2, CR3, CRM, CRW, DCR, DCS, DNG, ERF, FFF, IIQ, KDC, MEF, MOS, MRW, NEF, NRW, ORF, ORI, PEF, QTK, RAF, RAW, RW2, RWL, SR2, SRF and SRW files are read with `rawler`, and tested with real files from Canon, Nikon, Sony, Fujifilm, Olympus, Panasonic and Google cameras, and a Samsung Galaxy phone. EyeCanSee develops the sensor data; when the result comes out as a single flat colour (as with the DNG files of some Samsung Galaxy phones), it shows the full-size preview stored in the file instead. Photos are shown upright, also the portrait ones of old Canon cameras (CRW, such as the EOS 300D), which note the turn only in Canon's own data. A RAW file from a camera `rawler` does not know yet (such as the Phase One P40+) is developed by ImageMagick where it was built with LibRaw (as in the Fedora package); otherwise only the small preview inside the file is shown.

### Videos and audio

Played by the video engine (see [Playing videos](video.md)):

- **Videos**: MP4, M4V, MOV, MKV, WebM, AVI, WMV, MPG and MPEG, MTS and M2TS, 3GP, FLV, OGV, and INSV and LRV from action cameras.
- **Audio**: M4A, MP3, Opus, Ogg and OGA, FLAC, WAV, MKA, AMR, AC-3 and E-AC-3, AAC, WMA.

### 3D models

Shown in the grid as a picture and in the viewer in 3D, drawn by the graphics card (see [3D models](model-3d.md)).

| Format | Extensions | Read with |
|---|---|---|
| STL (binary and text) | stl | `stl_io` |
| OBJ (the colours and pictures of its materials, `.mtl`, or the colours of its vertices) | obj | `tobj` |
| PLY (text and binary, triangle strips, vertex colours; point clouds as dots) | ply | `ply-rs-bw` |
| glTF 2.0 (GLB, and .gltf with its data inside or in the files next to it; triangles, strips, fans, lines and points; also compressed with Draco or meshopt, or quantized) | glb, gltf | `gltf`, `draco-oxide`, `meshopt` |
| 3MF (components, Production extension, colours and pictures) | 3mf | `zip`, `roxmltree` |
| IFC 2x3, 4 and 4.3 (building models; spaces and nearly invisible parts left out) | ifc | `ifc-lite` |
| FBX (binary and text, material and vertex colours, pictures) | fbx | `ufbx` |
| STEP, IGES, BREP (solids of mechanical CAD programs, colours, assemblies) | step, stp, iges, igs, brep, brp | Open CASCADE 8.0.1 |
| Collada (also zipped; skinned meshes in their rest pose) | dae, zae | `roxmltree` |
| 3DS (3D Studio, material colours and pictures) | 3ds | EyeCanSee |
| OFF and COFF | off | EyeCanSee |
| AMF (also zipped, constellations, colours) | amf | `roxmltree` |
| VRML 97 and X3D (XML and classic, also gzipped; inlined files next to it; line and point sets, texts) | wrl, wrz, x3d, x3dv, x3dz, x3dvz | EyeCanSee, `roxmltree` |

STEP, IGES and BREP need the CAD engine, an add-on downloaded once (8 MB on Windows, 11 MB on Linux): see [Add-on components](components.md).

### CAD drawings

Seen from above (the model space, and the layouts with **L**) and shown as a picture, drawn again sharp at every zoom (see [CAD drawings](cad.md)).

| Format | Extensions | Read with |
|---|---|---|
| DWG (R13 to 2018 and later; line types, weights, hatch patterns, layouts, SHX fonts) | dwg | `acadrust`, `encoding_rs` |
| DXF (text and binary, R12 to 2018 and later; line types, weights, hatch patterns, layouts, SHX fonts) | dxf | `acadrust`, `encoding_rs` |
| HP-GL and HP-GL/2 plot files (also inside PCL; pens, widths, line types, user units, arcs, labels) | plt, hpgl, hpg, hgl | EyeCanSee |

## Saving images

**Save as…** and **Convert…** write these formats, with the same choices on Linux and Windows: all the encoders are part of the program (see [Saving in another format](save-as.md)).

| Format | Options | Lossless | Keeps metadata | Written with |
|---|---|:-:|:-:|---|
| JPEG | quality 1–100; colour detail 4:4:4, 4:2:2, 4:2:0 or greys only; progressive; optimised Huffman tables; arithmetic coding; lossless JPEG | lossless mode | ✓ | libjpeg-turbo |
| PNG | compression level 0–9; filter (adaptive, none, Sub, Up, Average, Paeth) | ✓ | ✓ | `image` |
| WebP | lossy (quality 1–100, sharper colour edges) or lossless (near-lossless 0–100); effort 0–6 | lossless mode | ✓ | libwebp |
| AVIF | quality 1–100; transparency quality 1–100; speed 1–10; 8 or 10 bits per channel; RGB or YCbCr inside the file | – | ✓ | ravif (rav1e) |
| TIFF | compression: none, LZW, Deflate (fast, normal, maximum), PackBits; horizontal predictor | ✓ | ✓ | `tiff` |
| BMP | – | ✓ | – | `image` |
| GIF | colour reduction quality 1–30 (at most 256 colours) | – | – | `image` |
| QOI | – | ✓ | – | `image` |
| ICO | – (at most 256 × 256 pixels: larger images are reduced) | up to 256 × 256 | – | `image` |
| TGA | RLE compression | ✓ | – | `image` |
| PNM | – (PPM, PGM or, with transparency, PAM; 8 or 16 bits) | ✓ | – | `image` |
| OpenEXR | – (32-bit float) | ✓ | – | `image` |
| Radiance HDR | – | – | – | `image` |
| Farbfeld | – (16 bits per channel) | ✓ | – | `image` |

- JPEG and Radiance HDR have no transparency: transparent parts are laid on white. Arithmetic coding and lossless JPEG make smaller or exact files that many programs cannot open.
- After a turn, **Save** writes back over the original only JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF and QOI, and PSD files (see [Rotating and flipping](rotate.md)): an ICO holds several sizes, OpenEXR and HDR more than what is shown.
- EyeCanSee does not write HEIC (because of the HEVC patents) nor JPEG XL (no encoder in the program yet).

## Metadata in saved images

The metadata (EXIF, XMP, IPTC, colour profile) can be kept in JPEG, PNG, WebP, AVIF and TIFF.

- **EXIF** is carried over from JPEG, PNG, WebP, TIFF, HEIC, AVIF, PSD and the RAW files of every maker (tested on 49 cameras; Kodak KDC files give only part of theirs). From RAW and HEIC files the camera, lens, shot, GPS, author and copyright fields go in, not the maker note, whose offsets point into the original file. The tags that describe the RAW data itself (sensor, strips, previews, DNG calibration) are left out.
- The **colour profile** comes from JPEG, PNG, WebP, TIFF, GIF, HEIC, AVIF and PSD. A RAW file gets the sRGB profile of the pixels EyeCanSee develops from it (from the CC0 Compact ICC Profiles by Clinton Ingram).
- **XMP and IPTC** (author, copyright, licence, descriptions, keywords) are kept in JPEG, PNG and TIFF; WebP and AVIF have no place for IPTC, so its fields go into the XMP. They are read from JPEG, PNG, WebP, TIFF, HEIC, AVIF, PSD and PSB, and the XMP also from RAW files. The sizes and orientation in the XMP are updated; the thumbnails stored by cameras and Photoshop and the Lightroom and Camera Raw settings (a crop and adjustments already in the pixels) are left out.
- The rare formats read through ImageMagick give the EXIF, XMP, IPTC and colour profile ImageMagick finds in them (the profile only when it is RGB, like the pixels written); GIMP files (XCF) only their colour profile, since GIMP keeps the rest in a form of its own.
- The EXIF orientation is set to upright, since the saved pixels are already turned.

## Saving videos, 3D models and drawings

- **Videos**: MP4, MKV, WebM, MOV, AVI and GIF, through FFmpeg (see [Converting videos](convert-video.md) and the [video formats](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/video-formats.md)).
- **3D models**: STL, OBJ, PLY and glTF (GLB), keeping as much of the original as the format holds; animations, cameras and lights are not kept, and pictures larger than 2048 pixels are saved at 2048 (see [Exporting 3D models](model-export.md)).
- **CAD drawings**: as pictures or SVG (the model space, drawn again at the size chosen), and DWG to DXF or the other way round (the whole drawing; a file that would lose part of the drawing is not saved) (see [CAD drawings](cad.md)).
