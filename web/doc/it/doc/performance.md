# Prestazioni

`fkv_bench` Prova i carichi di scrittura, lettura e scansione su uno spazio di chiave fisso.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Gli indicatori includono velocità effettiva, latenza in ogni fase e picco RSS. rilascio del flusso di lavoro rilascio x86_64 /
`fkv_bench` binario per aarch64.

## Abbinamento a tre con RocksDB/fjall

`crate/perf/fkv_compare` Utilizza lo stesso stage driver e la stessa codifica della chiave (pedice big-endian da 8 byte alla fine,
L'ordine del dizionario è coerente con l'ordine dei pedici) Test fkv, RocksDB, fjall: RocksDB superato
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` La libreria condivisa compilata viene caricata con dlopen (lo stesso C
ABI, la build di Rust non si collega a RocksDB), fjall utilizza `fjall 3` direttamente. Ciascuno dei tre motori apre una libreria.
Avanzare sulla stessa libreria nell'ordine delle fasi: il caricamento in blocco crea i dati, quindi len/lettura punto/lettura intervallo/
Le eliminazioni riguardano tutte gli stessi dati.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Salta la terza colonna se non c'è RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

I numeri per la matrice della home page possono provenire solo da una corsa tracciabile:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "Esegui CI <id>"
```

`perf.yml` di GitHub Actions fa esattamente questo (installa `librocksdb-dev`, programma gli adattatori, esegui
Matrice di righe da 25 milioni, carica artefatto e riempi direttamente la pagina quando `backfill=true`).

### calibro

- **Rapporto dati/cache**: 25.000.000 di righe × 116 B ≈ 2766 MiB di dati, 256 MiB di cache per motore,
  ≈ 11:1, in linea con la revisione ufficiale di RocksDB di 97 GiB di dati/6 GiB di cache (≈ 16:1).
- **Stessa quantità di cache**: cache delle pagine di fkv (indice/dati stesso pool), cache dei blocchi di RocksDB
  (`cache_index_and_filter_blocks=true`, dimensione del blocco 4 KiB), cache dei blocchi di fjall
  I budget sono tutti impostati sullo stesso numero di byte.
- **Calibro di persistenza (la colonna di confronto utilizza la configurazione predefinita di RocksDB, senza allineamento)**: fkv 1×pagina dati + per batch
  1×manifest `fdatasync` (isomorfo a SyncData + AppendManifest di C++ EloqStore,
  Nessuno dei due ha un interruttore di spegnimento); la colonna RocksDB utilizza la **configurazione predefinita** (WAL attivo, `WriteOptions.sync=false`,
  Nessun fsync sequenziale), fjall utilizza anche il valore predefinito (`PersistMode::Buffer`). Pertanto la fase di scrittura (scrittura singola/
  La differenza tra scritture batch) è la **differenza di forza di persistenza**, non la differenza di implementazione, che deve essere indicata nella pagina insieme al numero.
  Se vuoi analizzare "chi è più veloce con la stessa forza di sincronizzazione", aggiungi `--align-fsync` (apri RocksDB con sync=true,
  fjall utilizza SyncAll), questa opzione non viene utilizzata per le matrici pubblicate esternamente.
- **Due driver fkv**: `--fkv-driver store` (predefinito, connesso direttamente a un singolo `Store` all'interno del processo e
  RocksDB / fjall sono anche chiamate di libreria) e `--fkv-driver runtime` (modalità di distribuzione: un processo
  `--fkv-shards` thread di shard, le richieste vengono instradate per chiave, allineate con la lettura e la scrittura pubblica di C++ EloqStore
  interfaccia). Ogni richiesta di quest'ultimo deve passare attraverso la coda + riattivazione cross-thread, nativa (macOS, file IO passa attraverso il pool di thread)
  Il tempo di lettura è di circa 20 µs/ora, ovvero un ordine di grandezza più lento rispetto alla connessione diretta in-process; molto più economico su Linux/io_uring, ma
  Ancora un salto. La matrice della pagina utilizza il calibro del negozio e il calibro del runtime viene utilizzato per "modulo di distribuzione rispetto a C++
  Confronto di "Interfaccia pubblica" (la lettura a thread singolo del runtime fkv è di circa 50 kops/s, C++ EloqStore FFI a thread singolo
  frammento di circa 21 kops/s).
- **Lettura del calibro di concorrenza**: numero di thread lato fkv = `min(readers, --read-max-concurrency)`, predefinito
  Il limite superiore è 4 (allineato alla macchina di valutazione vCPU), **non** "un thread per lettore" - il livello superato aumenta solo la coda;
  Cache per thread = 256 MiB/numero di thread (l'importo totale è fisso, non distrutto dai lettori) e utilizzato
  `FuturesUnordered` Mantieni `--read-window` (default 16) richieste al volo. Il lato RocksDB lo è
  `readers` Ciascuno `std::thread` legge simultaneamente; loop a thread singolo sul lato fjall (ignorando l'attrezzatura dei lettori,
  Solo come colonna di riferimento). `--no-fkv-read-threads` può degenerare in "catene asincrone N interlacciate su un singolo runtime".
  Si tratta della profondità della pipeline, non della concorrenza, e non è paragonabile tra i motori.
- **len()**: nessuno dei tre motori dispone di interfacce di conteggio O(1). Questa riga rappresenta la velocità di conteggio dell'intera tabella, non la ricerca O(1).