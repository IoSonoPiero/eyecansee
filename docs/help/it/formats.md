# Formati supportati

Che cosa apre EyeCanSee, e che cosa sa salvare. Le tabelle complete, con ogni estensione e opzione, sono nel repository (in inglese): [formati delle immagini](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/formats.md) e [formati video](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/video-formats.md).

## Aprire

- **Foto**: JPEG, PNG, WebP, HEIC e HEIF, AVIF, JPEG XL, TIFF, GIF (il primo fotogramma), BMP.
- **RAW delle fotocamere**: ARW, CR2, CR3, CRW, DNG, NEF, NRW, ORF, PEF, RAF, RW2, SRW, 3FR, IIQ, ERF, KDC, MEF, MOS, MRW e altri. EyeCanSee mostra l'anteprima a grandezza piena salvata dalla fotocamera; se non c'è, sviluppa i dati del sensore.
- **Vettoriali**: SVG e SVGZ.
- **Grafica e HDR**: PSD (l'immagine unita), ORA, OpenEXR, HDR, DDS, ICNS, QOI, TGA, ICO, CUR, PCX, JPEG 2000, PNM, XPM, XBM, SGI, WBMP, OTB, MPO, JPS.
- **Tramite ImageMagick**: PSB, XCF, PFM, Sun Raster, PICT, MIFF, MNG, JNG, DPX, Cineon, FITS e altri formati meno comuni (XWD solo su Linux). ImageMagick apre anche i PSD che il lettore interno non gestisce.
- **Video**: MP4, MOV, MKV, WebM, AVI e altri, riprodotti dal motore video (vedi [Riprodurre i video](video.md)).
- **Audio**: M4A, MP3, Opus, Ogg, FLAC, WAV, MKA e altri.
- **Modelli 3D**: STL, 3MF, OBJ, PLY, glTF (GLB), FBX, Collada (DAE, ZAE), 3DS, OFF, AMF, VRML, X3D, IFC; STEP, IGES e BREP con il motore CAD (vedi [Modelli 3D](model-3d.md)).
- **Disegni CAD**: DWG, DXF, HP-GL e HP-GL/2 (PLT) (vedi [Disegni CAD](cad.md)).

Le foto vengono raddrizzate secondo il loro orientamento EXIF. L'estensione è solo un indizio: un file con quella sbagliata (per esempio una foto HEIC dell'iPhone rinominata `.jpg`) viene riconosciuto dal contenuto.

## Salvare le immagini

**Salva come…** e **Converti e ridimensiona** scrivono JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF, QOI, ICO, TGA, PNM, OpenEXR, Radiance HDR e Farbfeld, con le stesse scelte su Linux e su Windows (vedi [Salvare in un altro formato](save-as.md)).

- I metadati (EXIF, XMP, IPTC, profilo colore) si possono mantenere in JPEG, PNG, WebP, AVIF e TIFF.
- Dopo una rotazione, **Salva** riscrive sopra l'originale solo JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF e QOI, e i file PSD (vedi [Ruotare e riflettere](rotate.md)).
- EyeCanSee non scrive HEIC (per i brevetti di HEVC) né JPEG XL (nel programma non c'è ancora un codificatore).

## Salvare video, modelli 3D e disegni

- **Video**: MP4, MKV, WebM, MOV, AVI e GIF, tramite FFmpeg (vedi [Convertire i video](convert-video.md)).
- **Modelli 3D**: STL, OBJ, PLY e glTF (GLB) (vedi [Esportare i modelli 3D](model-export.md)).
- **Disegni CAD**: come immagini o SVG, e da DWG a DXF o il contrario (vedi [Disegni CAD](cad.md)).
