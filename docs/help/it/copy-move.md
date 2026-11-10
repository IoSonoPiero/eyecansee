# Copiare e spostare

Copia o sposta file e cartelle, con tutto quello che contengono, in un'altra cartella: sullo stesso disco, su un altro disco, su un telefono o su un server, anche da un dispositivo all'altro. Spostare una cartella del telefono sul computer, o foto dal computer a un server, funziona allo stesso modo.

## Copiare o spostare

1. Seleziona i file o le cartelle (vedi [Selezionare i file](selection.md)).
2. Premi **Ctrl+C** per copiarli, oppure **Ctrl+X** per tagliarli (cioè spostarli). Puoi anche fare clic su ![Copia](icon://copy) o ![Taglia](icon://scissors) nella barra strumenti, o usare il menù **Modifica** o il clic destro.
3. Apri la cartella dove devono andare: sul computer, su un disco, su un telefono o su un server (vedi [Cartelle e pannello laterale](folders.md)).
4. Premi **Ctrl+V**, oppure fai clic su ![Incolla](icon://clipboard-paste), oppure scegli **Modifica > Incolla**.

Per incollare direttamente in una cartella senza aprirla, fai clic destro su di essa (nella griglia o nell'albero) e scegli **Incolla in questa cartella**. Nel visore **Ctrl+C** e **Ctrl+X** prendono l'immagine mostrata.

## Che cosa vedi

- Gli elementi copiati hanno un segno blu ![Copiato](icon://color-copied) nell'angolo della loro casella; quelli tagliati uno arancione con le forbici ![Tagliato](icon://color-cut), e diventano più chiari. La barra di stato dice quanti aspettano Ctrl+V.
- Durante la copia un riquadro in basso a destra mostra il file che sta copiando, la cartella di arrivo, quanti file e quanti dati sono fatti, la percentuale, la velocità degli ultimi secondi e il tempo che manca. Con tante cartelle, prima dice quanti elementi ha trovato guardandoci dentro.
- La cartella di arrivo si riempie mentre la copia va avanti.

## Quando un nome c'è già

Se nella destinazione c'è già un elemento con lo stesso nome, EyeCanSee chiede che cosa fare:

- **Sostituisci**: il file che c'è viene sostituito. Per una cartella il pulsante è **Unisci**: il suo contenuto va nella cartella che c'è già, e i file con lo stesso nome vengono sostituiti.
- **Salta**: questo elemento non viene copiato.
- **Tieni entrambi**: quello nuovo diventa "nome (2)".
- **Annulla**: non viene incollato niente.

Con più elementi in questa situazione, spunta **Fai lo stesso per tutti gli altri elementi con un nome già presente** per rispondere una volta sola.

## Annullare

**Annulla** nel riquadro ferma la copia. I file già copiati restano; il file in corso non resta scritto a metà. Verso un telefono, Annulla aspetta la fine del file in corso, perché un telefono che riceve mezzo file smette di rispondere.

## Da sapere

- Ogni copia o taglio si incolla una volta sola: per incollare di nuovo, copia di nuovo.
- Se nel frattempo copi qualcos'altro in un altro programma, Ctrl+V e Incolla non incollano più gli elementi copiati in EyeCanSee.
- Incollate nella cartella da cui vengono, le copie diventano "nome (2)"; uno spostamento non fa niente.
- Una copia che non riesce, per esempio perché il disco è pieno, non lascia niente scritto a metà: il file in corso viene tolto, e un file che stava sostituendo resta com'era. Per questo, mentre un file viene sostituito, sul disco serve spazio per tutte e due le versioni.
- Su questo computer un file sostituito mantiene permessi, attributi e collegamenti; un file in sola lettura non viene sostituito, e il messaggio lo dice.
- Uno spostamento sullo stesso disco cambia solo il posto, all'istante. Tra dispositivi diversi EyeCanSee prima copia tutto e cancella gli originali solo quando la copia è riuscita; gli originali non vanno nel cestino.
- Le copie dentro lo stesso telefono le fa il telefono stesso.
- Una cartella non si può incollare dentro se stessa. Non si può incollare niente in un cestino o nell'elenco delle memorie di un telefono, e gli elementi dentro un cestino non si possono copiare.
- Una cartella spostata si porta dietro il suo cestino di EyeCanSee (vedi [Eliminare e cestino](delete.md)); una copia lo lascia fuori.
