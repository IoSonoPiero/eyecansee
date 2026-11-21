# Ruotare e riflettere

Una foto scattata di traverso si può raddrizzare, e un'immagine si può riflettere come in uno specchio. Nel visore vedi prima la modifica e la salvi quando vuoi; nella griglia la modifica viene salvata subito, anche su tante foto insieme.

## I comandi

Sono nel menù **Immagine**, e nel menù del clic destro sotto **Trasforma**:

- ![Rifletti orizzontalmente](icon://square-split-horizontal) **Rifletti orizzontalmente** e ![Rifletti verticalmente](icon://square-split-vertical) **Rifletti verticalmente**;
- **Ruota di 90° in senso orario** (**Ctrl+R**), ![Ruota di 90° in senso orario](icon://rotate-cw-square) nella barra strumenti;
- **Ruota di 90° in senso antiorario** (**Ctrl+Maiusc+R**), ![Ruota di 90° in senso antiorario](icon://rotate-ccw-square) nella barra strumenti;
- **Ruota di 180°**.

Funzionano sulle foto, e le tre rotazioni anche sui video (vedi sotto); su modelli 3D e disegni CAD sono grigi. Per ruotare una foto di un angolo qualsiasi, per esempio per raddrizzare un orizzonte, usa **Immagine > Ruota e ritaglia…**: vedi [Ritagliare e ruotare](editor-crop.md).

## Nel visore

1. Apri la foto e ruotala o rifletti: vedi subito il risultato, e la barra di stato dice **Modificata, non salvata**.
2. **File > Salva** (**Ctrl+S**) la salva sopra l'originale, dopo aver chiesto. **File > Salva come…** (**Ctrl+Maiusc+S**) salva un file nuovo nel formato che scegli, e lascia l'originale com'è (vedi [Salva come](save-as.md)).

**Ctrl+Z** annulla l'ultima rotazione o riflessione non ancora salvata, **Ctrl+Y** (o **Ctrl+Maiusc+Z**) la rifà. Tornata com'è sul disco, la foto non è più segnata come modificata.

Se lasci la foto senza salvare (un'altra foto, ritorno alla griglia, un segnalibro, chiusura di EyeCanSee), EyeCanSee chiede prima: **Salva…**, **Salva come…**, **Non salvare**, oppure **Annulla** per restare sulla foto.

## Nella griglia

Seleziona una o più foto e scegli una rotazione o un ribaltamento dal menù **Immagine**, dalla barra strumenti o dal menù del clic destro, oppure premi **Ctrl+R** o **Ctrl+Maiusc+R**. Una finestra dice quante foto verranno salvate sopra gli originali e dove vanno gli originali; **Salva** lo fa, **Annulla** (o **Esc**) non fa niente. Qui **Invio** non conferma, così non si sovrascrivono tante foto per sbaglio. Poi le miniature vengono rifatte, e un messaggio con il titolo di ciò che è stato fatto (per esempio **Immagini ruotate**) dice quante foto sono state salvate. Se alcune sono state saltate, le nomina: le prime dieci, poi quante altre; ![Copia il messaggio](icon://copy) copia il messaggio con tutti i nomi.

Le foto in un formato che EyeCanSee non sa scrivere (HEIC, RAW…) vengono escluse, e la finestra dice quante sono. Vengono saltati anche i file animati, quelli con più pagine e le foto con una mappa di profondità (i ritratti di alcuni telefoni), perché salvandoci sopra quelle parti andrebbero perse; le foto in movimento dei telefoni vengono ruotate insieme al loro video, e le foto Ultra HDR insieme al loro HDR. Quando l'unica foto scelta non si può salvare sopra sé stessa, la finestra spiega perché e propone **Salva come…**, già ruotata.

## Video

Un video si ruota come una foto, nel visore o nella griglia, e si salva sopra l'originale allo stesso modo, con l'originale nel cestino. Nel visore gira subito, e **Ctrl+Z** e **Ctrl+Y** annullano e rifanno una rotazione non ancora salvata, come per una foto. Non viene ricodificato: cambia solo la rotazione che leggono i lettori, quindi bastano pochi secondi, non si perde niente e i metadati restano. Funziona su MP4, MOV, MKV e WebM, con un FFmpeg (vedi [Convertire i video](convert-video.md)): se EyeCanSee non ne ha uno da usare, propone di scaricarlo quando salvi. Un video non si può riflettere così: per rifletterlo usa **Converti…**. Sugli altri video (AVI, MPEG…) rotazioni e riflessi sono grigi, nel menù del clic destro tutto **Trasforma**: si riflettono e si ruotano con **Converti…**. **Ruota e ritaglia…** e **Modifica immagine…** lavorano solo sulle immagini: su un video anche queste due voci sono grigie, e puntandole si legge perché.

## Salvare sopra l'originale

- L'originale va dove lo manderebbe un'eliminazione, di solito il cestino, così puoi recuperarlo. Vedi [Eliminare e cestino](delete.md).
- Un JPEG viene ruotato senza perdere qualità quando larghezza e altezza sono multipli di 16 pixel; altrimenti viene risalvato con la qualità che aveva. Anche WebP, AVIF e GIF vengono risalvati, e possono perdere un po' di qualità. La conferma lo dice.
- I metadati (data, fotocamera, posizione) restano nel file.
- Le foto Ultra HDR dei telefoni tengono il loro HDR, e le foto in movimento il loro video, mostrato ruotato come la foto (il video non viene ricodificato).
- Un'immagine che per essere risalvata chiederebbe più di metà della memoria del computer (per esempio un HEIC, un TIFF a 16 bit o un PNG di centinaia di megapixel, su un computer da 8 GB) non viene salvata subito: una finestra dice quanta memoria le serve e propone **Apri comunque** (il computer può allora rallentare molto, oppure il sistema può chiudere EyeCanSee); la scelta vale per quel file finché EyeCanSee resta aperto. Nella griglia un'immagine così viene lasciata fuori, e il messaggio alla fine la nomina. Un JPEG ruotato senza perdere qualità non chiede mai: non viene decodificato.
- In una cartella dove salvare sopra un file non è sicuro (per esempio un telefono aperto dal file manager), Salva spiega perché e propone **Salva come…**.
