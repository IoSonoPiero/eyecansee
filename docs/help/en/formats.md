# Supported formats

What EyeCanSee opens, and what it can save. The full tables, with every extension and option, are in the repository: [image formats](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/formats.md) and [video formats](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/video-formats.md).

## Opening

- **Photos**: JPEG, PNG, WebP, HEIC and HEIF, AVIF, JPEG XL, TIFF, GIF (the first frame), BMP.
- **Camera RAW**: ARW, CR2, CR3, CRW, DNG, NEF, NRW, ORF, PEF, RAF, RW2, SRW, 3FR, IIQ, ERF, KDC, MEF, MOS, MRW and others. EyeCanSee shows the full-size preview stored by the camera; if there is none, it develops the sensor data.
- **Vector**: SVG and SVGZ.
- **Graphics and HDR**: PSD (the merged image), ORA, OpenEXR, HDR, DDS, ICNS, QOI, TGA, ICO, CUR, PCX, JPEG 2000, PNM, XPM, XBM, SGI, WBMP, OTB, MPO, JPS.
- **Through ImageMagick**: PSB, XCF, PFM, Sun Raster, PICT, MIFF, MNG, JNG, DPX, Cineon, FITS and other less common formats (XWD on Linux only). ImageMagick also opens the PSD files the built-in reader does not handle.
- **Videos**: MP4, MOV, MKV, WebM, AVI and others, played by the video engine (see [Playing videos](video.md)).
- **Audio**: M4A, MP3, Opus, Ogg, FLAC, WAV, MKA and others.
- **3D models**: STL, 3MF, OBJ, PLY, glTF (GLB), FBX, Collada (DAE, ZAE), 3DS, OFF, AMF, VRML, X3D, IFC; STEP, IGES and BREP with the CAD engine (see [3D models](model-3d.md)).
- **CAD drawings**: DWG, DXF, HP-GL and HP-GL/2 (PLT) (see [CAD drawings](cad.md)).

Photos are turned upright using their EXIF orientation. The extension is only a hint: a file with the wrong one (for example an iPhone HEIC photo renamed to `.jpg`) is recognised from its content.

## Saving images

**Save as…** and **Convert and resize** write JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF, QOI, ICO, TGA, PNM, OpenEXR, Radiance HDR and Farbfeld, with the same choices on Linux and Windows (see [Saving in another format](save-as.md)).

- The metadata (EXIF, XMP, IPTC, colour profile) can be kept in JPEG, PNG, WebP, AVIF and TIFF.
- After a turn, **Save** writes back over the original only JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF and QOI, and PSD files (see [Rotating and flipping](rotate.md)).
- EyeCanSee does not write HEIC (because of the HEVC patents) nor JPEG XL (no encoder in the program yet).

## Saving videos, 3D models and drawings

- **Videos**: MP4, MKV, WebM, MOV, AVI and GIF, through FFmpeg (see [Converting videos](convert-video.md)).
- **3D models**: STL, OBJ, PLY and glTF (GLB) (see [Exporting 3D models](model-export.md)).
- **CAD drawings**: as pictures or SVG, and DWG to DXF or the other way round (see [CAD drawings](cad.md)).
