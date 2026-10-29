# Windows smette di comunicare con il telefono

*[English](phone-stall.en.md)*

## Cosa succede

Su Windows, mentre un telefono è aperto in EyeCanSee, ogni operazione su di esso può fermarsi per circa **sei minuti**: la cartella non si carica, una copia non va avanti, ed EyeCanSee dice "Windows ha perso il collegamento con il telefono". Anche gli altri programmi che aprono il telefono in quei minuti restano in attesa. Poi tutto riparte da solo.

Succede subito dopo un cambiamento fatto sul telefono stesso: una foto appena scattata (la fotocamera la salva in più passaggi), un file cancellato dalla galleria. È stato visto con telefoni Xiaomi e Samsung.

## Cosa fare

- **Stacca e ricollega il cavo**, poi scegli **Trasferimento file** sul telefono: Windows torna subito a comunicare con il telefono.
- Oppure aspetta qualche minuto: si sblocca da solo.

Il telefono e i file sono a posto. Solo un file che stava venendo copiato **verso** il telefono quando stacchi il cavo resta lì a metà: va ricopiato.

## Perché

Su Windows i programmi raggiungono il telefono attraverso il driver di Windows (Windows Portable Devices, lo stesso che usa Esplora file). È quel driver a smettere di rispondere, e nessun programma senza diritti di amministratore può farlo ripartire: staccare il cavo è il modo per azzerarlo. Microsoft non ne ha documentato la causa. Blocchi simili di cinque o sei minuti, anche in Esplora file, sono segnalati da anni dopo aver rinominato o cancellato file su un telefono.

Su Linux EyeCanSee comunica con il telefono direttamente via USB, senza quel driver.

## Segnalazioni

- [mtp-rs #52](https://github.com/vdavid/mtp-rs/issues/52): il blocco dopo una cancellazione, segnalato da EyeCanSee alla libreria che usa per i telefoni.
- [mtp-rs #43](https://github.com/vdavid/mtp-rs/issues/43): un'attesa di 135 s, con Esplora file che non rispondeva nello stesso momento.
- [XDA: blocco MTP rinominando](https://xdaforums.com/t/q-mtp-mode-freeze-on-renaming-file-folder.2155438/): Esplora file fermo per 5 min 37 s dopo una rinomina (in inglese).
- [Microsoft Q&A: rinominare un file via MTP è lento](https://learn.microsoft.com/en-us/answers/questions/2576612/it-takes-long-time-to-rename-a-file-under-mtp-in-w?forum=windows-all) (in inglese).
