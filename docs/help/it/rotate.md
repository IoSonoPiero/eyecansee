# Ruotare e riflettere

Una foto scattata di traverso si può raddrizzare, e un'immagine si può riflettere come in uno specchio. Nel visore vedi prima la modifica e la salvi quando vuoi; nella griglia la modifica viene salvata subito, anche su tante foto insieme.

## I comandi

Sono nel menù **Immagine**, e nel menù del clic destro sotto **Trasforma**:

- ![Rifletti orizzontalmente](icon://square-split-horizontal) **Rifletti orizzontalmente** e ![Rifletti verticalmente](icon://square-split-vertical) **Rifletti verticalmente**;
- **Ruota di 90° in senso orario** (**Ctrl+R**, nel visore), ![Ruota di 90° in senso orario](icon://rotate-cw-square) nella barra strumenti;
- **Ruota di 90° in senso antiorario** (**Ctrl+Maiusc+R**, nel visore), ![Ruota di 90° in senso antiorario](icon://rotate-ccw-square) nella barra strumenti;
- **Ruota di 180°**.

Funzionano solo sulle foto: su video, modelli 3D e disegni CAD sono grigi. Per ruotare un video vedi [Convertire i video](convert-video.md). Per ruotare una foto di un angolo qualsiasi, per esempio per raddrizzare un orizzonte, usa **Immagine > Ruota e ritaglia…**: vedi [Ritagliare e ruotare](editor-crop.md).

## Nel visore

1. Apri la foto e ruotala o rifletti: vedi subito il risultato, e la barra di stato dice **Modificata, non salvata**.
2. **File > Salva** (**Ctrl+S**) la salva sopra l'originale, dopo aver chiesto. **File > Salva come…** (**Ctrl+Maiusc+S**) salva un file nuovo nel formato che scegli, e lascia l'originale com'è (vedi [Salvare in un altro formato](save-as.md)).

Se lasci la foto senza salvare (un'altra foto, ritorno alla griglia, un segnalibro, chiusura di EyeCanSee), EyeCanSee chiede prima: **Salva…**, **Salva come…**, **Non salvare**, oppure **Annulla** per restare sulla foto.

## Nella griglia

Seleziona una o più foto e scegli una rotazione o un ribaltamento dal menù **Immagine**, dalla barra strumenti o dal menù del clic destro. Una finestra dice quante foto verranno salvate sopra gli originali e dove vanno gli originali; **Salva** lo fa, **Annulla** (o **Esc**) non fa niente. Qui **Invio** non conferma, così non si sovrascrivono tante foto per sbaglio. Poi le miniature vengono rifatte.

Le foto in un formato che EyeCanSee non sa scrivere (HEIC, RAW…) vengono escluse, e la finestra dice quante sono. Vengono saltati anche i file animati, quelli con più pagine e le foto in movimento; le foto Ultra HDR dei telefoni vengono ruotate insieme al loro HDR, e al loro video se sono anche foto in movimento. Quando l'unica foto scelta non si può salvare sopra sé stessa, la finestra spiega perché e propone **Salva come…**, già ruotata.

## Salvare sopra l'originale

- L'originale va dove lo manderebbe un'eliminazione, di solito il cestino, così puoi recuperarlo. Vedi [Eliminare e cestino](delete.md).
- Un JPEG viene ruotato senza perdere qualità quando larghezza e altezza sono multipli di 16 pixel; altrimenti viene risalvato con la qualità che aveva. Anche WebP, AVIF e GIF vengono risalvati, e possono perdere un po' di qualità. La conferma lo dice.
- I metadati (data, fotocamera, posizione) restano nel file.
- Le foto Ultra HDR dei telefoni tengono il loro HDR, e il loro video se sono anche foto in movimento.
- In una cartella dove salvare sopra un file non è sicuro (per esempio un telefono aperto dal file manager), Salva spiega perché e propone **Salva come…**.
