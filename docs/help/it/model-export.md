# Esportare i modelli 3D

EyeCanSee sa salvare un modello 3D in un altro formato 3D, per esempio per aprire un modello di un programma CAD in un programma di stampa 3D, o per mandare un modello in un formato che leggono più programmi.

## Come esportare

1. Seleziona uno o più modelli 3D nella griglia (oppure aprine uno nel visore).
2. Fai clic destro e scegli **Esporta come modello 3D…**, oppure usa **File > Esporta come modello 3D…**.
3. Nella finestra scegli il **formato**: **STL**, **OBJ**, **PLY** o **glTF (GLB)** (viene proposto l'ultimo usato), poi fai clic su **Esporta**. Invio fa lo stesso; Esc o **Annulla** chiude la finestra senza esportare.

Ogni file nuovo viene salvato accanto al suo originale, con lo stesso nome e la nuova estensione. Se quel nome è già preso si aggiunge " (2)", per esempio `staffa (2).stl`. La barra di stato dice il nome che ha preso.

Mentre i modelli vengono esportati, un riquadro in basso a destra mostra il modello in esportazione, quale è su quanti, e a che punto è l'esportazione. **Annulla** nel riquadro (raggiungibile anche con **F6**) la ferma: i modelli già esportati restano, il file che si stava scrivendo viene tolto e gli altri non vengono esportati. Una seconda esportazione avviata nel frattempo aspetta la prima e si unisce al suo riquadro. Se una copia o una conversione finisce vicino a lei, lo dicono in un solo resoconto, **Lavori finiti**, dove un'esportazione senza niente da dire ha la sua riga (**Esportazione finita** con quanti modelli, o **Esportazione annullata** con quanti sono stati esportati prima di **Annulla**). Se un modello non si riesce a esportare, o il file nuovo non è proprio come l'originale (un glTF che mette una sua parte più volte nella gerarchia, cosa che il formato non permette: il file nuovo la contiene una volta sola), lo dice un messaggio alla fine dell'esportazione, **Esportazione finita**: un solo messaggio per tutti i modelli, che dice quanti non sono stati esportati e quanti con una modifica, con i loro nomi in ordine (i primi dieci, poi quanti altri) e, accanto a ogni modello non esportato, il perché in breve quando si sa. ![Copia il messaggio](icon://copy) lo copia con tutti i nomi e il motivo completo.

EyeCanSee apre i file OBJ fino a 10 milioni di triangoli, i file STL binari e GLB fino a 20 milioni e i file PLY binari fino a 50 milioni (vedi [Modelli 3D](model-3d.md)). Un modello con più triangoli di quanti ne permette il formato scelto si può esportare lo stesso, ma EyeCanSee non potrà aprire il file nuovo: per un modello già letto nel visore lo dice la finestra sotto il formato, e il messaggio **Esportazione finita** elenca gli altri, ciascuno con il numero di triangoli che permette il suo nuovo formato.

Se sono selezionati anche file di altro tipo, la voce è segnata con **\*** e lavora solo sui modelli. I disegni CAD si esportano allo stesso modo, con **File > Esporta disegno…**: vedi [Disegni CAD](cad.md).

## Cosa tiene ogni formato

EyeCanSee tiene tutto quello dell'originale che il nuovo formato sa contenere:

- **glTF (GLB)** tiene le immagini sulle superfici (texture), le normali e i materiali (quanto sono lisci o metallici, il rilievo, le ombre nelle pieghe, la luce propria).
- **OBJ** tiene le immagini sulle superfici, le loro coordinate, le normali, i colori e i materiali (quanto sono lisci o metallici, il rilievo, la luce propria). Le immagini si salvano come file PNG accanto al modello e i materiali in un file `.mtl` con il nome del modello (`staffa.mtl`, `staffa.png`, oppure `staffa-1.png`, `staffa-2.png`… con più immagini): vanno tenuti nella stessa cartella dell'OBJ. Un nome già preso lì riceve " (2)", così nessun file viene sovrascritto. I triangoli che si toccano condividono i loro punti, così gli altri programmi vedono una superficie sola.
- **PLY** tiene le normali e i colori, sia sui punti (quelli che leggono quasi tutti i programmi) sia su ogni triangolo.
- **STL** tiene un colore per ogni triangolo.

Dove un formato non sa contenere un'immagine, i colori vengono presi dall'immagine, così il modello non esce grigio. Quello che il nuovo formato non sa contenere lo dice il messaggio **Esportazione finita**: i modelli esportati senza le loro immagini (STL e PLY) e quelli esportati senza tutti i loro materiali (STL e PLY non hanno posto per loro; OBJ non ne ha per le immagini di ruvidità, metallo e ombre nelle pieghe). glTF (GLB) li tiene tutti.

I nomi delle parti, i gruppi e la loro gerarchia, e la trasparenza non si tengono in nessun formato: EyeCanSee legge un modello come un unico insieme di triangoli.

## Dov'è l'alto

Ogni file viene scritto con l'asse verso l'alto che si aspettano gli altri programmi: Z per STL e PLY, Y per OBJ e glTF. Così il modello si apre in piedi anche lì. Per questo gli assi nell'angolo del visore possono essere diversi da quelli dell'originale.

## Unità di misura

glTF usa i metri. Un modello il cui file dice la sua unità (STEP e IGES in millimetri, 3MF e AMF come dicono) viene convertito in metri quando si esporta in glTF, così tiene la sua misura: un pezzo di 100 mm diventa 0,1 m. STL, OBJ e PLY non hanno un posto per l'unità: i numeri restano come sono, e l'unità viene scritta in cima al file solo come commento, che gli altri programmi non leggono, e nemmeno EyeCanSee: riaperti, quei file hanno misure senza unità.

## Da sapere

- Per salvare invece un modello come immagine, vedi [Modelli 3D](model-3d.md) (**Ctrl + Maiusc + S** nel visore).
- I modelli che EyeCanSee apre sono elencati in [Formati supportati](formats.md).
