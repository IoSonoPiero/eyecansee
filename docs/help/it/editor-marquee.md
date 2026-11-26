# Selezionare un'area

![Selezione](icon://square-dashed) **Selezione** (M) sceglie una parte dell'immagine, per copiarla, tagliarla, incollarla di nuovo come oggetto o applicarci un filtro.

## Scegliere una zona

1. Scegli ![Selezione](icon://square-dashed) **Selezione** (M).
2. Trascina sull'immagine. Tieni premuto Maiusc per un quadrato.
3. Un contorno tratteggiato mostra la zona scelta. I suoi lati seguono i bordi dei pixel dell'immagine, così una copia ha i pixel come sono.

Un clic sull'immagine, o Esc, lascia la zona. Esc mentre disegni una zona nuova (col tasto ancora premuto) rimette la zona scelta prima. La lascia anche scegliere un altro strumento.

## Copiare, tagliare e incollare

La barra delle proprietà ha tre pulsanti, che hanno anche il loro tasto:

- ![Copia la zona](icon://copy) **Copia la zona** (**Ctrl + C**): copia la zona come la vedi, la foto con ciò che ci hai disegnato sopra. Va anche negli appunti, così puoi incollarla in un altro programma.
- ![Taglia la zona](icon://scissors) **Taglia la zona** (**Ctrl + X**): copia la zona, poi la copre con un riquadro pieno. Il riquadro prende il colore scelto per gli angoli vuoti in [Ritagliare e ruotare](editor-crop.md), o il bianco se non ce n'è uno. Come per la [Copertura](editor-cover.md), nel file salvato non resta niente di ciò che nasconde.
- ![Incolla](icon://clipboard-paste) **Incolla** (**Ctrl + V**): rimette la zona come oggetto nuovo, un po' spostata rispetto a dov'era. È selezionata, con Seleziona scelto, così la trascini al suo posto, la ridimensioni e la ruoti: vedi sotto.

## Incollare da un altro programma

**Ctrl + V** incolla anche un'immagine copiata in un altro programma. Va al centro del risultato, grande al massimo quattro quinti, come oggetto da spostare, ridimensionare e ruotare. Se dopo aver copiato una zona qui hai copiato qualcosa in un altro programma, viene incollata quell'immagine. Se non c'è un'immagine da incollare, lo dice la barra di stato. Un'immagine con un lato più lungo di 20.000 pixel, o più grande di 200 megapixel in tutto, non si incolla: un messaggio ne dice la misura.

## Ruotare, riflettere e misurare un pezzo incollato

Un pezzo incollato, da qui o da un altro programma, è un'immagine che puoi sistemare come vuoi. Selezionalo (con Seleziona, o subito dopo averlo incollato) e:

- Trascinalo per spostarlo.
- Trascina un angolo per ridimensionarlo. Mantiene le proporzioni; tieni premuto **Maiusc** mentre trascini per cambiare larghezza e altezza separatamente.
- Trascina la maniglia tonda sopra di lui per ruotarlo di un angolo qualsiasi. Tieni premuto **Maiusc** per girarlo a scatti di 15°.

Il resto è nella barra delle proprietà:

- **Rotazione**: il suo angolo sullo schermo, da −180° a 180°.
- ![Ruota di 90° in senso antiorario](icon://rotate-ccw-square) e ![Ruota di 90° in senso orario](icon://rotate-cw-square): un quarto di giro del solo pezzo.
- ![Rifletti orizzontalmente](icon://square-split-horizontal) e ![Rifletti verticalmente](icon://square-split-vertical): rispecchiano il pezzo da sinistra a destra o dall'alto in basso, come lo vedi.
- Larghezza e altezza in pixel. Con **Mantieni le proporzioni** acceso (come all'inizio) l'altro lato segue; spento, ogni lato cambia da solo, anche trascinando un angolo (allora Maiusc le mantiene).
- **Opacità**.
- ![Porta in primo piano](icon://to-front) **Porta in primo piano** e ![Porta dietro](icon://to-back) **Porta dietro**: sopra tutti gli altri oggetti, o sotto.

Ognuno di questi è un passo di Annulla. Per quante volte tu ruoti, rifletta o ridimensioni il pezzo, viene sempre ridisegnato dai pixel incollati, quindi non perde qualità, e il file salvato è quello che vedi.

## Filtri su una zona

Con una zona scelta, il menù ![Filtri](icon://wand-sparkles) **Filtri** applica il filtro solo lì. Vedi [Filtri](editor-filters.md).

## Da sapere

- Senza una zona scelta, **Ctrl + C** copia tutto il risultato e **Ctrl + X** non fa niente.
- La zona copiata è un'immagine: gli oggetti che contiene non si cambiano più uno per uno.
- Su Linux con GNOME o X11 gli altri programmi ricevono al massimo 16 MB di una copia (un'immagine PNG), circa 12 megapixel di una foto. Una copia più grande si incolla solo nella finestra da cui viene, e lo dice la barra di stato.
