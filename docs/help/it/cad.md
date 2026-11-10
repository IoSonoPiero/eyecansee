# Disegni CAD

I disegni di AutoCAD (**DWG** e **DXF**) e i file per plotter (**HP-GL** e **HP-GL/2**: `.plt`, `.hpgl`) compaiono nella griglia con un'immagine del disegno come miniatura, e si aprono nel visore come una foto.

<!-- shot: cad | panoramica | a DWG floor plan open in the viewer on its dark background, zoomed in a little, with the X Y axes in the bottom left corner and the status bar showing the size in metres, the layout name and the zoom -->

## Guardare un disegno

Fai doppio clic su un disegno, oppure selezionalo e premi **Invio**. Il disegno si vede dall'alto, come nello spazio modello di un programma CAD, sul suo sfondo scuro, con i suoi colori, tipi di linea e spessori. I layer spenti e congelati non si vedono.

| Tasto o mouse | Cosa fa |
|---|---|
| Rotellina, + e − | Ingrandiscono e rimpiccioliscono |
| Trascinare | Sposta il disegno |
| 0 | Tutto il disegno |
| →, ← | File successivo, precedente |
| L, Maiusc+L | Layout successivo, precedente |
| Ctrl+Maiusc+S | Salva come immagine |
| F11 | Schermo intero |
| Esc, doppio clic | Torna alla griglia |

Quando la vista si ferma, la parte sullo schermo viene ridisegnata dal disegno, così linee sottili e testi piccoli restano nitidi a ogni ingrandimento. Il pulsante **Tasti** nella barra di stato elenca i tasti.

## Layout

Un disegno con layout (spazio carta) mostra nella barra di stato **Modello** o il nome del layout. **L** passa al layout successivo e **Maiusc+L** a quello prima, ciascuno col suo foglio, il cartiglio e le finestre sul modello.

## La barra di stato

Un disegno non ha pixel: la barra di stato mostra la sua misura nella sua unità (come la scrive il disegno; millimetri per un file per plotter; una misura grande passa a un'unità più grande, per esempio metri invece di millimetri), il layout e l'ingrandimento. In basso a sinistra gli assi X (rosso) e Y (verde) del disegno mostrano dove puntano.

## Testi e immagini

I testi si scrivono con i font SHX del disegno quando i file dei font sono accanto al disegno o in una cartella `fonts` lì accanto (su Windows anche in un'installazione di AutoCAD); altrimenti con un font del sistema. Le immagini collegate al disegno si vedono quando i loro file sono accanto.

## Salvare un disegno come immagine o nell'altro formato AutoCAD

- Nel visore, **Ctrl+Maiusc+S** o **Salva come…** nel menù del clic destro salva quello che vedi (il modello o il layout mostrato) come immagine. Vedi [Salvare in un altro formato](save-as.md).
- **Strumenti > Esporta disegno…** (anche nel menù del clic destro) salva i disegni selezionati nella griglia, tutti insieme. Scegli il **formato**: un formato di immagine, **SVG** (nitido a ogni ingrandimento, senza livelli né blocchi), oppure **DWG** / **DXF** (tutto il disegno nell'altro formato AutoCAD, con livelli, blocchi, layout e stili). Per le immagini scegli il **Lato lungo** in pixel: il disegno viene ridisegnato a quella misura, quindi resta nitido anche grande. Poi scegli dove vanno i file, come in [Convertire e ridimensionare le immagini](convert.md), e premi **Esporta**.

Immagini e SVG mostrano lo spazio modello, non i layout. Un disegno salvato come DWG o DXF viene riletto e confrontato con l'originale: se qualcosa andrebbe perso non viene salvato, e il riepilogo alla fine lo dice.

Le rotazioni e i ribaltamenti del menù Immagine non valgono per i disegni.
