# Ydeevne

`fkv_bench` Test skrive, læse og scanne belastninger på et fast nøglerum.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatorer inkluderer gennemløb, latens på hvert trin og peak RSS. frigiv workflow release x86_64 /
`fkv_bench` binær for aarch64.

## Tre-vejs matchup med RocksDB/fjall

`crate/perf/fkv_compare` Brug den samme scenedriver og den samme nøglekodning (8-byte big-endian subscript i slutningen,
Ordbogsrækkefølgen stemmer overens med sænket rækkefølge) Test fkv, RocksDB, fjall: RocksDB bestået
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Det kompilerede delte bibliotek er indlæst med dlopen (samme C
ABI, Rust build linker ikke til RocksDB), fjall bruger `fjall 3` direkte. Hver af de tre motorer åbner et bibliotek.
Gå videre på det samme bibliotek i rækkefølgen af ​​trin: bulk load skaber data, og derefter len / punkt læsning / område læsning /
Sletninger påvirker alle de samme data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Spring den tredje kolonne over, hvis der ikke er nogen RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Numrene for startsidematrixen kan kun komme fra én sporbar kørsel:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --maskine "..." --kilde "CI run <id>"
```

GitHub Actions' `perf.yml` gør præcis dette (installer `librocksdb-dev`, program adaptere, kør
25M rækkematrix, upload artefakter og udfyld siden direkte, når `backfill=true`).

### kaliber

- **Data/cache-forhold**: 25.000.000 rækker × 116 B ≈ 2766 MiB data, 256 MiB cache pr. motor,
  ≈ 11:1, tilpasset RocksDBs officielle gennemgang af 97 GiB data/6 GiB cache (≈ 16:1).
- **Samme mængde cache**: fkvs sidecache (indeks/data samme pulje), RocksDBs blokcache
  (`cache_index_and_filter_blocks=true`, blokstørrelse 4 KiB), fjalls egen blokcache
  Budgetterne er alle indstillet til det samme antal bytes.
- **Persistens kaliber (sammenligningskolonnen bruger RocksDB standardkonfigurationen uden justering)**: fkv 1×dataside + pr. batch
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Ingen af ​​dem har en slukket kontakt); RocksDB-kolonnen bruger **standardkonfigurationen** (WAL on, `WriteOptions.sync=false`,
  Ingen sekventiel fsync), fjall bruger også standarden (`PersistMode::Buffer`). Derfor skrivefasen (enkeltskrivning /
  Forskellen mellem batchskrivninger) er **persistensstyrkeforskellen**, ikke implementeringsforskellen, som skal angives på siden sammen med nummeret.
  Hvis du vil analysere "hvem er hurtigere under samme synkroniseringsstyrke", tilføj `--align-fsync` (åbn RocksDB med sync=true,
  fjall bruger SyncAll), denne mulighed bruges ikke til eksternt publicerede matricer.
- **To fkv-drivere**: `--fkv-driver store` (standard, direkte forbundet til en enkelt `Store` i processen, og
  RocksDB / fjall er også bibliotekskald) og `--fkv-driver runtime` (implementeringstilstand: en proces
  `--fkv-shards` shard-tråde, anmodninger dirigeres med nøgle, tilpasset offentlig læsning og skrivning af C++ EloqStore
  grænseflade). Hver anmodning fra sidstnævnte skal gå gennem køen + cross-thread wake-up, native (macOS, fil IO går gennem trådpuljen)
  Læsetiden er ca. 20 µs/tid, hvilket er en størrelsesorden langsommere end direkte forbindelse i processen; meget billigere på Linux/io_uring, men
  Stadig et spring. Sidematrixen bruger butikskaliberen, og runtime-kaliberen bruges til "deployment form vs C++
  Sammenligning af "Public Interface" (fkv runtime single-thread læsning er omkring 50 kops/s, C++ EloqStore FFI single-thread
  skår ca. 21 kops/s).
- **Læs samtidighedskaliber**: fkv sidegevindnummer = `min(readers, --read-max-concurrency)`, standard
  Den øvre grænse er 4 (justeret med evalueringsmaskinens vCPU), **ikke** "én tråd pr. læser" - det overskredet niveau øger kun køen;
  Cache pr. tråd = 256 MiB / antal tråde (det samlede beløb er fast, ikke makuleret med læsere) og brugt
  `FuturesUnordered` Hold `--read-window` (standard 16) anmodninger på farten. RocksDB side er
  `readers` Hver `std::thread` læser samtidigt; enkeltgevind løkke på fjallsiden (ignorerer læsernes gear,
  Kun som referencekolonne). `--no-fkv-read-threads` kan degenerere tilbage til "sammenflettede N asynkrone kæder på en enkelt runtime".
  Det er pipeline dybde, ikke samtidighed, og er ikke sammenlignelig på tværs af motorer.
- **len()**: Ingen af ​​de tre motorer har O(1)-tællegrænseflader. Denne linje er den fulde tabeltællehastighed, ikke O(1)-opslag.