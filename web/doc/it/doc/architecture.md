# Architettura

## tempo di esecuzione

Ogni frammento ha un thread del sistema operativo esclusivo e un runtime Compio. La richiesta entra attraverso la coda di ingresso,
Ammissione tramite macchina a stati shard, completata tramite ricevuta one-shot.

## magazzinaggio

Il motore utilizza B+tree copy-on-write. pagina dati, pagina indice, catena di overflow e
Il manifest viene serializzato in file di segmenti. il ramo è identificato da `(branch, term)` ed è posseduto da ciascuno
manifest e snapshot di mappatura.

## IO

`fkv_io` Definisce la caratteristica IO asincrona. `fkv_io_compio` Implementato con compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` fornisce memoria e backend di inserimento errori, `fkv_object_s3` fornisce
Backend dell'oggetto S3.

## persistenza

Il manifest registra la pagina root, la root TTL, i metadati del ramo e la mappatura. Alla riapertura
Per prima cosa riproduci il manifest, quindi ricostruisci il page mapper e quindi inizia a servire le richieste.