# Prestaties

`fkv_bench` Test schrijf-, lees- en scanbelastingen op een vaste sleutelruimte.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indicatoren zijn onder meer doorvoer, latentie in elke fase en piek-RSS. werkstroom vrijgave x86_64 /
`fkv_bench` binair voor aarch64.

## Drievoudige match met RocksDB/fjall

`crate/perf/fkv_compare` Gebruik dezelfde stage driver en dezelfde sleutelcodering (8-byte big-endian subscript aan het einde,
Woordenboekvolgorde komt overeen met subscriptvolgorde) Test fkv, RocksDB, fjall: RocksDB geslaagd
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` De gecompileerde gedeelde bibliotheek is geladen met dlopen (dezelfde C
ABI, Rust build linkt niet naar RocksDB), fjall gebruikt `fjall 3` rechtstreeks. Elk van de drie motoren opent een bibliotheek.
Ga verder met dezelfde bibliotheek in de volgorde van fasen: bulkladen creëert gegevens en vervolgens len / punt lezen / bereik lezen /
Verwijderingen hebben allemaal invloed op dezelfde gegevens.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Sla de derde kolom over als er geen RocksDB is
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

De cijfers voor de startpaginamatrix kunnen slechts afkomstig zijn uit één traceerbare run:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI-run <id>"
```

GitHub Actions' `perf.yml` doet precies dit (installeer `librocksdb-dev`, programmeer adapters, voer
25 miljoen rijenmatrix, artefact uploaden en de pagina direct aanvullen wanneer `backfill=true`).

### kaliber

- **Data/cache-verhouding**: 25.000.000 rijen × 116 B ≈ 2766 MiB aan gegevens, 256 MiB cache per engine,
  ≈ 11:1, in lijn met RocksDB’s officiële beoordeling van 97 GiB-gegevens/6 GiB-cache (≈ 16:1).
- **Dezelfde hoeveelheid cache**: fkv's paginacache (index/data dezelfde pool), RocksDB's blokcache
  (`cache_index_and_filter_blocks=true`, blokgrootte 4 KiB), fjall's eigen blokcache
  De budgetten zijn allemaal ingesteld op hetzelfde aantal bytes.
- **Persistentiekaliber (de vergelijkingskolom gebruikt de standaardconfiguratie van RocksDB, zonder uitlijning)**: fkv 1×datapagina + per batch
  1×manifest `fdatasync` (Isomorf met SyncData + AppendManifest van C++ EloqStore,
  Geen van beide heeft een uitschakelaar); de RocksDB-kolom gebruikt de **standaardconfiguratie** (WAL aan, `WriteOptions.sync=false`,
  Geen sequentiële fsync), gebruikt fjall ook de standaardwaarde (`PersistMode::Buffer`). Daarom is de schrijffase (single write /
  Het verschil tussen batchschrijfbewerkingen) is het **verschil in persistentiesterkte**, niet het implementatieverschil, dat samen met het nummer op de pagina moet worden vermeld.
  Als je wilt analyseren "wie sneller is onder dezelfde synchronisatiesterkte", voeg dan `--align-fsync` toe (open RocksDB met sync=true,
  fjall gebruikt SyncAll), wordt deze optie niet gebruikt voor extern gepubliceerde matrices.
- **Twee fkv-stuurprogramma's**: `--fkv-driver store` (standaard, direct verbonden met een enkele `Store` binnen het proces, en
  RocksDB / fjall zijn ook bibliotheekaanroepen) en `--fkv-driver runtime` (implementatiemodus: een proces
  `--fkv-shards` shard-threads, verzoeken worden gerouteerd per sleutel, afgestemd op openbaar lezen en schrijven van C++ EloqStore
  interface). Elk verzoek van laatstgenoemde moet via de wachtrij + cross-thread wake-up gaan, native (macOS, bestands-IO gaat door de threadpool)
  De leestijd bedraagt ​​ongeveer 20 µs/tijd, wat een orde van grootte langzamer is dan directe verbinding tijdens het proces; veel goedkoper op Linux/io_uring, maar
  Nog steeds een sprong. De paginamatrix gebruikt het winkelkaliber en het runtimekaliber wordt gebruikt voor "implementatieformulier versus C++
  Vergelijking van "Public Interface" (fkv runtime single-thread lezen is ongeveer 50 kops/s, C++ EloqStore FFI single-thread
  scherf ongeveer 21 kops/s).
- **Concurrency-kaliber lezen**: fkv-zijdethreadnummer = `min(readers, --read-max-concurrency)`, standaard
  De bovengrens is 4 (uitgelijnd met de vCPU van de evaluatiemachine), **niet** "één thread per lezer" - het overschreden niveau verhoogt alleen de wachtrij;
  Cache per thread = 256 MiB / aantal threads (het totale aantal staat vast, niet versnipperd met lezers) en gebruikt
  `FuturesUnordered` Bewaar `--read-window` (standaard 16) verzoeken direct. RocksDB-kant is
  `readers` Elke `std::thread` leest tegelijkertijd; lus met enkele schroefdraad aan de hele zijde (waarbij de uitrusting van de lezer wordt genegeerd,
  Alleen als referentiekolom). `--no-fkv-read-threads` kan terug degenereren naar "interleaved N asynchrone ketens op één runtime".
  Dat is pijplijndiepte, geen gelijktijdigheid, en is niet vergelijkbaar tussen motoren.
- **len()**: Geen van de drie motoren heeft O(1)-telinterfaces. Deze regel is de telsnelheid van de volledige tabel, niet de O(1)-opzoekopdracht.