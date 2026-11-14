# Formati supportati

Che cosa apre EyeCanSee, e che cosa sa salvare. Tutto funziona allo stesso modo su Linux e su Windows, tranne dove una riga dice altro. Contenitori, codifiche e codificatori per convertire i video sono elencati nei [formati video](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/video-formats.md) (in inglese).

## Aprire

### Immagini

| Formato | Estensioni | Letto con |
|---|---|---|
| JPEG | jpg, jpeg, jfif | libjpeg-turbo, `image` |
| MPO, JPS (più immagini e stereo: la prima immagine) | mpo, jps | `image` |
| PNG (animato: il primo fotogramma) | png, apng | `image` |
| WebP | webp | `image` |
| GIF (il primo fotogramma) | gif | `image` |
| BMP | bmp | `image` |
| TIFF | tif, tiff | `image` |
| ICO, CUR | ico, cur | `image` |
| TGA | tga | `image` |
| QOI | qoi | `image` |
| PNM | pbm, pgm, ppm, pnm, pam | `image` |
| OpenEXR | exr | `image` |
| Radiance HDR | hdr | `image` |
| Farbfeld | ff | `image` |
| HEIC, HEIF | heic, heif, hif | libheif con libde265 |
| AVIF | avif | libheif con dav1d |
| JPEG XL | jxl | `jxl-oxide` |
| SVG | svg, svgz | `resvg` |
| Photoshop (l'immagine unita) | psd | `psd` |
| JPEG 2000 | jp2, j2k, j2c, jpf, jpx | `hayro-jpeg2000` |
| DDS | dds | `image-extras` |
| OpenRaster | ora | `image-extras` |
| PCX | pcx | `image-extras` |
| SGI | sgi, rgb, rgba, bw | `image-extras` |
| WBMP, XBM, XPM, OTB | wbmp, xbm, xpm, otb | `image-extras` |
| Icona Apple | icns | `image-extras` |
| Documento grande di Photoshop | psb | ImageMagick |
| GIMP | xcf | ImageMagick |
| Sun raster | ras, sun | ImageMagick |
| PICT | pict, pct | ImageMagick |
| MIFF | miff | ImageMagick |
| MNG, JNG | mng, jng | ImageMagick |
| DPX, Cineon | dpx, cin | ImageMagick |
| FITS | fits | ImageMagick |
| PFM | pfm | ImageMagick |
| PGX, WPG, VIFF, VICAR | pgx, wpg, viff, vicar | ImageMagick |
| XWD (solo col pacchetto per Fedora, non nell'AppImage, nel Flatpak né su Windows) | xwd | ImageMagick |

Le foto vengono raddrizzate secondo il loro orientamento EXIF. L'estensione è solo un indizio: un file con quella sbagliata (per esempio una foto HEIC dell'iPhone rinominata `.jpg`) viene riconosciuto dal contenuto. Un file che il lettore del suo formato non riesce a leggere viene riprovato con ImageMagick, che apre anche i PSD che il lettore interno non gestisce.

### RAW delle fotocamere

I file 3FR, ARI, ARW, CR2, CR3, CRM, CRW, DCR, DCS, DNG, ERF, FFF, IIQ, KDC, MEF, MOS, MRW, NEF, NRW, ORF, ORI, PEF, QTK, RAF, RAW, RW2, RWL, SR2, SRF e SRW si leggono con `rawler`, provato con file veri di fotocamere Canon, Nikon, Sony, Fujifilm, Olympus, Panasonic e Google e di un telefono Samsung Galaxy. EyeCanSee sviluppa i dati del sensore; quando il risultato esce di un solo colore uniforme (come con i DNG di alcuni telefoni Samsung Galaxy), mostra invece l'anteprima a grandezza piena salvata nel file. Le foto si vedono dritte, anche quelle verticali delle vecchie Canon (CRW, come la EOS 300D), che annotano la rotazione solo nei dati propri di Canon. Un RAW di una fotocamera che `rawler` non conosce ancora (come la Phase One P40+) viene sviluppato da ImageMagick dove è compilato con LibRaw (come nel pacchetto di Fedora); altrimenti si vede solo la piccola anteprima dentro il file.

### Video e audio

Riprodotti dal motore video (vedi [Riprodurre i video](video.md)):

- **Video**: MP4, M4V, MOV, MKV, WebM, AVI, WMV, MPG e MPEG, MTS e M2TS, 3GP, FLV, OGV, e INSV e LRV delle action cam.
- **Audio**: M4A, MP3, Opus, Ogg e OGA, FLAC, WAV, MKA, AMR, AC-3 ed E-AC-3, AAC, WMA.

### Modelli 3D

Mostrati nella griglia come immagine e nel visore in 3D, disegnati dalla scheda video (vedi [Modelli 3D](model-3d.md)).

| Formato | Estensioni | Letto con |
|---|---|---|
| STL (binario e testo) | stl | `stl_io` |
| OBJ (i colori e le immagini dei suoi materiali, `.mtl`, o i colori dei vertici) | obj | `tobj` |
| PLY (testo e binario, strisce di triangoli, colori dei vertici; nuvole di punti come puntini) | ply | `ply-rs-bw` |
| glTF 2.0 (GLB, e .gltf con i dati dentro o nei file accanto; triangoli, strisce, ventagli, linee e punti; anche compresso con Draco o meshopt, o quantizzato) | glb, gltf | `gltf`, `draco-oxide`, `meshopt` |
| 3MF (componenti, estensione Production, colori e immagini) | 3mf | `zip`, `roxmltree` |
| IFC 2x3, 4 e 4.3 (modelli di edifici; spazi e parti quasi invisibili lasciati fuori) | ifc | `ifc-lite` |
| FBX (binario e testo, colori dei materiali e dei vertici, immagini) | fbx | `ufbx` |
| STEP, IGES, BREP (solidi dei programmi CAD meccanici, colori, assiemi) | step, stp, iges, igs, brep, brp | Open CASCADE 8.0.1 |
| Collada (anche compresso in zip; maglie con scheletro nella posa di riposo) | dae, zae | `roxmltree` |
| 3DS (3D Studio, colori e immagini dei materiali) | 3ds | EyeCanSee |
| OFF e COFF | off | EyeCanSee |
| AMF (anche compresso in zip, costellazioni, colori) | amf | `roxmltree` |
| VRML 97 e X3D (XML e classico, anche compressi con gzip; file inclusi accanto; insiemi di linee e di punti, testi) | wrl, wrz, x3d, x3dv, x3dz, x3dvz | EyeCanSee, `roxmltree` |

STEP, IGES e BREP richiedono il motore CAD, un componente aggiuntivo da scaricare una volta sola (8 MB su Windows, 11 MB su Linux): vedi [Componenti aggiuntivi](components.md).

### Disegni CAD

Visti dall'alto (lo spazio modello, e i layout con **L**) e mostrati come immagine, ridisegnata nitida a ogni ingrandimento (vedi [Disegni CAD](cad.md)).

| Formato | Estensioni | Letto con |
|---|---|---|
| DWG (da R13 a 2018 e successivi; tipi di linea, spessori, retini, layout, font SHX) | dwg | `acadrust`, `encoding_rs` |
| DXF (testo e binario, da R12 a 2018 e successivi; tipi di linea, spessori, retini, layout, font SHX) | dxf | `acadrust`, `encoding_rs` |
| File di stampa HP-GL e HP-GL/2 (anche dentro PCL; penne, spessori, tipi di linea, unità utente, archi, etichette) | plt, hpgl, hpg, hgl | EyeCanSee |

## Salvare le immagini

**Salva come…** e **Converti…** scrivono questi formati, con le stesse scelte su Linux e su Windows: tutti i codificatori fanno parte del programma (vedi [Salvare in un altro formato](save-as.md)).

| Formato | Opzioni | Senza perdita | Mantiene i metadati | Scritto con |
|---|---|:-:|:-:|---|
| JPEG | qualità 1–100; dettaglio del colore 4:4:4, 4:2:2, 4:2:0 o solo grigi; progressivo; tabelle di Huffman ottimizzate; codifica aritmetica; JPEG senza perdita | modalità senza perdita | ✓ | libjpeg-turbo |
| PNG | livello di compressione 0–9; filtro (adattivo, nessuno, Sub, Up, Average, Paeth) | ✓ | ✓ | `image` |
| WebP | con perdita (qualità 1–100, bordi dei colori più nitidi) o senza perdita (quasi senza perdita 0–100); impegno 0–6 | modalità senza perdita | ✓ | libwebp |
| AVIF | qualità 1–100; qualità della trasparenza 1–100; velocità 1–10; 8 o 10 bit per canale; RGB o YCbCr dentro il file | – | ✓ | ravif (rav1e) |
| TIFF | compressione: nessuna, LZW, Deflate (veloce, normale, massima), PackBits; predittore orizzontale | ✓ | ✓ | `tiff` |
| BMP | – | ✓ | – | `image` |
| GIF | qualità della riduzione dei colori 1–30 (al massimo 256 colori) | – | – | `image` |
| QOI | – | ✓ | – | `image` |
| ICO | – (al massimo 256 × 256 pixel: le immagini più grandi vengono ridotte) | fino a 256 × 256 | – | `image` |
| TGA | compressione RLE | ✓ | – | `image` |
| PNM | – (PPM, PGM o, con la trasparenza, PAM; 8 o 16 bit) | ✓ | – | `image` |
| OpenEXR | – (virgola mobile a 32 bit) | ✓ | – | `image` |
| Radiance HDR | – | – | – | `image` |
| Farbfeld | – (16 bit per canale) | ✓ | – | `image` |

- JPEG e Radiance HDR non hanno la trasparenza: le parti trasparenti vengono appoggiate sul bianco. La codifica aritmetica e il JPEG senza perdita danno file più piccoli o identici che molti programmi non sanno aprire.
- Dopo una rotazione, **Salva** riscrive sopra l'originale solo JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF e QOI, e i file PSD (vedi [Ruotare e riflettere](rotate.md)): un ICO contiene più dimensioni, OpenEXR e HDR più di quello che si vede.
- EyeCanSee non scrive HEIC (per i brevetti di HEVC) né JPEG XL (nel programma non c'è ancora un codificatore).

## Metadati nelle immagini salvate

I metadati (EXIF, XMP, IPTC, profilo colore) si possono mantenere in JPEG, PNG, WebP, AVIF e TIFF.

- L'**EXIF** passa da JPEG, PNG, WebP, TIFF, HEIC, AVIF, PSD e dai RAW di ogni marca (provato su 49 fotocamere; i Kodak KDC danno solo una parte del loro). Dai RAW e dagli HEIC passano i campi di fotocamera, obiettivo, scatto, GPS, autore e copyright, non la nota del produttore, i cui riferimenti puntano dentro il file originale. I tag che descrivono i dati RAW stessi (sensore, strisce, anteprime, calibrazione DNG) restano fuori.
- Il **profilo colore** viene da JPEG, PNG, WebP, TIFF, GIF, HEIC, AVIF e PSD. Un RAW riceve il profilo sRGB dei pixel che EyeCanSee ne sviluppa (dai Compact ICC Profiles CC0 di Clinton Ingram).
- **XMP e IPTC** (autore, copyright, licenza, descrizioni, parole chiave) si mantengono in JPEG, PNG e TIFF; WebP e AVIF non hanno posto per l'IPTC, quindi i suoi campi vanno nell'XMP. Si leggono da JPEG, PNG, WebP, TIFF, HEIC, AVIF, PSD e PSB, e l'XMP anche dai RAW. Dimensioni e orientamento nell'XMP vengono aggiornati; le miniature salvate da fotocamere e Photoshop e le impostazioni di Lightroom e Camera Raw (un ritaglio e regolazioni già nei pixel) restano fuori.
- I formati rari letti tramite ImageMagick danno l'EXIF, l'XMP, l'IPTC e il profilo colore che ImageMagick ci trova (il profilo solo se è RGB, come i pixel scritti); i file di GIMP (XCF) solo il loro profilo colore, perché GIMP tiene il resto in una forma sua.
- L'orientamento EXIF viene messo dritto, perché i pixel salvati sono già ruotati.

## Salvare video, modelli 3D e disegni

- **Video**: MP4, MKV, WebM, MOV, AVI e GIF, tramite FFmpeg (vedi [Convertire i video](convert-video.md) e i [formati video](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/video-formats.md)).
- **Modelli 3D**: STL, OBJ, PLY e glTF (GLB), con tutto ciò che il formato sa contenere dell'originale; animazioni, telecamere e luci non si mantengono, e le immagini più grandi di 2048 pixel si salvano a 2048 (vedi [Esportare i modelli 3D](model-export.md)).
- **Disegni CAD**: come immagini o SVG (lo spazio modello, ridisegnato alla dimensione scelta), e da DWG a DXF o il contrario (tutto il disegno; un file che perderebbe una parte del disegno non viene salvato) (vedi [Disegni CAD](cad.md)).
