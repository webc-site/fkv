# Leistung

`fkv_bench` Testen Sie Schreib-, Lese- und Scan-Lasten auf einem festen Schlüsselbereich.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Zu den Indikatoren gehören Durchsatz, Latenz in jeder Phase und Spitzen-RSS. Release-Workflow Release x86_64 /
`fkv_bench` Binärdatei für aarch64.

## Dreikampf mit RocksDB/fjall

`crate/perf/fkv_compare` Verwenden Sie denselben Stufentreiber und dieselbe Schlüsselcodierung (8-Byte-Big-Endian-Index am Ende,
Die Wörterbuchreihenfolge stimmt mit der Indexreihenfolge überein.) Test fkv, RocksDB, fjall: RocksDB bestanden
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Die kompilierte gemeinsam genutzte Bibliothek wird mit dlopen (dem gleichen C
ABI, Rust-Build stellt keine Verknüpfung zu RocksDB her), fjall verwendet `fjall 3` direkt. Jede der drei Engines öffnet eine Bibliothek.
Fahren Sie mit derselben Bibliothek in der Reihenfolge der Stufen fort: Durch Massenladen werden Daten erstellt, und dann werden Len / Punkt-Lesen / Bereichs-Lesen /
Löschungen betreffen alle dieselben Daten.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Überspringen Sie die dritte Spalte, wenn keine RocksDB vorhanden ist
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Die Zahlen für die Home-Matrix können nur aus einem nachvollziehbaren Lauf stammen:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions' `perf.yml` macht genau das (installieren `librocksdb-dev`, Programmadapter, ausführen
25 Millionen Zeilenmatrix, Artefakt hochladen und Seite direkt auffüllen, wenn `backfill=true`).

### Kaliber

- **Daten-/Cache-Verhältnis**: 25.000.000 Zeilen × 116 B ≈ 2766 MiB Daten, 256 MiB Cache pro Engine,
  ≈ 11:1, abgestimmt auf RocksDBs offizielle Überprüfung von 97 GiB Daten/6 GiB Cache (≈ 16:1).
- **Gleiche Menge an Cache**: Seitencache von fkv (Index/Daten gleicher Pool), Blockcache von RocksDB
  (`cache_index_and_filter_blocks=true`, Blockgröße 4 KiB), Fjalls eigener Block-Cache
  Die Budgets sind alle auf die gleiche Anzahl von Bytes eingestellt.
- **Persistenzkaliber (die Vergleichsspalte verwendet die RocksDB-Standardkonfiguration ohne Ausrichtung)**: fkv 1×Datenseite + pro Stapel
  1×manifest `fdatasync` (Isomorph zu SyncData + AppendManifest von C++ EloqStore,
  Keiner hat einen Aus-Schalter); Die RocksDB-Spalte verwendet die **Standardkonfiguration** (WAL aktiviert, `WriteOptions.sync=false`,
  Kein sequentielles fsync), fjall verwendet auch die Standardeinstellung (`PersistMode::Buffer`). Daher ist die Schreibphase (einzelnes Schreiben /
  Der Unterschied zwischen Batch-Schreibvorgängen ist der **Unterschied in der Persistenzstärke**, nicht der Implementierungsunterschied, der auf der Seite zusammen mit der Zahl angegeben werden muss.
  Wenn Sie analysieren möchten, „wer bei gleicher Synchronisationsstärke schneller ist“, fügen Sie `--align-fsync` hinzu (öffnen Sie RocksDB mit sync=true,
  Da fjall SyncAll verwendet, wird diese Option nicht für extern veröffentlichte Matrizen verwendet.
- **Zwei FKV-Treiber**: `--fkv-driver store` (Standard, direkt mit einem einzelnen `Store` innerhalb des Prozesses verbunden und
  RocksDB / fjall sind ebenfalls Bibliotheksaufrufe) und `--fkv-driver runtime` (Bereitstellungsmodus: ein Prozess
  `--fkv-shards` Shard-Threads, Anfragen werden nach Schlüssel weitergeleitet, abgestimmt auf öffentliches Lesen und Schreiben von C++ EloqStore
  Schnittstelle). Jede Anforderung des letzteren muss die Warteschlange + Thread-übergreifendes Aufwecken durchlaufen, nativ (MacOS, Datei-E/A geht durch den Thread-Pool).
  Die Lesezeit beträgt etwa 20 µs/Zeit und ist damit um eine Größenordnung langsamer als bei einer prozessinternen Direktverbindung. viel günstiger unter Linux/io_uring, aber
  Immer noch ein Sprung. Die Seitenmatrix verwendet das Store-Kaliber und das Laufzeitkaliber wird für „Bereitstellungsform vs. C++“ verwendet
  Vergleich der „öffentlichen Schnittstelle“ (fkv-Laufzeit-Single-Thread-Lesen beträgt etwa 50 Kops/s, C++ EloqStore FFI-Single-Thread).
  Shard etwa 21 Kops/s).
- **Parallelitätskaliber lesen**: FKV-Seiten-Thread-Nummer = `min(readers, --read-max-concurrency)`, Standard
  Die Obergrenze liegt bei 4 (ausgerichtet auf die vCPU der Evaluierungsmaschine), **nicht** „ein Thread pro Leser“ – der überschrittene Wert erhöht nur die Warteschlange;
  Cache pro Thread = 256 MiB/Anzahl der Threads (die Gesamtmenge ist festgelegt, nicht mit Lesern geschreddert) und wird verwendet
  `FuturesUnordered` Behalten Sie `--read-window` (Standard 16) Anfragen im laufenden Betrieb bei. RocksDB-Seite ist
  `readers` Jeder `std::thread` liest gleichzeitig; Single-Threaded-Schleife auf der Fjall-Seite (ohne Berücksichtigung der Leserausrüstung,
  Nur als Referenzspalte. `--no-fkv-read-threads` kann wieder zu „verschachtelten N asynchronen Ketten auf einer einzigen Laufzeit“ degenerieren.
  Das ist Pipeline-Tiefe, nicht Parallelität, und ist nicht über alle Engines hinweg vergleichbar.
- **len()**: Keine der drei Engines verfügt über O(1)-Zählschnittstellen. Diese Zeile ist die vollständige Tabellenzählrate, nicht die O(1)-Suche.