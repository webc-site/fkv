# Prestanda

`fkv_bench` Testa skriv-, läs- och skannaladdningar på ett fast nyckelutrymme.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatorer inkluderar genomströmning, latens i varje steg och topp RSS. release workflow release x86_64 /
`fkv_bench` binär för aarch64.

## Trevägsmatchning med RocksDB/fjall

`crate/perf/fkv_compare` Använd samma stegdrivrutin och samma nyckelkodning (8-byte big-endian subscript i slutet,
Ordboksordning överensstämmer med underskriftsordning) Test fkv, RocksDB, fjall: RocksDB godkänd
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Det kompilerade delade biblioteket laddas med dlopen (samma C
ABI, Rust build länkar inte till RocksDB), fjall använder `fjall 3` direkt. Var och en av de tre motorerna öppnar ett bibliotek.
Avancera på samma bibliotek i stegordningen: bulkbelastning skapar data, och sedan len / punktläsning / intervallläsning /
Alla raderingar påverkar samma data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Hoppa över den tredje kolumnen om det inte finns någon RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Siffrorna för startsidans matris kan bara komma från en spårbar körning:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --maskin "..." --källa "CI-körning <id>"
```

GitHub Actions `perf.yml` gör exakt detta (installera `librocksdb-dev`, programmera adaptrar, kör
25M radmatris, ladda upp artefakter och fyll sidan direkt när `backfill=true`).

### kaliber

- **Data/cache-förhållande**: 25 000 000 rader × 116 B ≈ 2766 MiB data, 256 MiB cache per motor,
  ≈ 11:1, i linje med RocksDB:s officiella granskning av 97 GiB-data/6 GiB-cache (≈ 16:1).
- **Samma mängd cache**: fkvs sidcache (index/data samma pool), RocksDB:s blockcache
  (`cache_index_and_filter_blocks=true`, blockstorlek 4 KiB), fjalls egen blockcache
  Budgetarna är alla inställda på samma antal byte.
- ** Persistens kaliber (jämförelsekolumnen använder RocksDB standardkonfiguration, utan justering)**: fkv 1×datasida + per batch
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Ingen av dem har en avstängningsknapp); kolumnen RocksDB använder **standardkonfigurationen** (WAL på, `WriteOptions.sync=false`,
  Ingen sekventiell fsync), fjall använder också standarden (`PersistMode::Buffer`). Därför skrivfasen (enkel skriv /
  Skillnaden mellan batchskrivningar) är **persistensstyrkeskillnaden**, inte implementeringsskillnaden, som måste anges på sidan tillsammans med numret.
  Om du vill analysera "vem är snabbare under samma synkroniseringsstyrka", lägg till `--align-fsync` (öppna RocksDB med sync=true,
  fjall använder SyncAll), det här alternativet används inte för externt publicerade matriser.
- **Två fkv-drivrutiner**: `--fkv-driver store` (standard, direkt ansluten till en enda `Store` i processen, och
  RocksDB / fjall är också biblioteksanrop) och `--fkv-driver runtime` (distributionsläge: en process
  `--fkv-shards` skärvtrådar, förfrågningar dirigeras med nyckel, anpassade till offentlig läsning och skrivning av C++ EloqStore
  gränssnitt). Varje begäran från den senare måste gå igenom kön + väckning över trådar, native (macOS, fil IO går genom trådpoolen)
  Avläsningstiden är cirka 20 µs/tid, vilket är en storleksordning långsammare än direktanslutning under processen; mycket billigare på Linux/io_uring, men
  Fortfarande ett hopp. Sidmatrisen använder butikskalibern, och runtime-kalibern används för "deployment form vs C++
  Jämförelse av "Public Interface" (fkv körtid entrådsavläsning är cirka 50 kops/s, C++ EloqStore FFI enkeltråd
  skärva ca 21 kops/s).
- **Läs concurrency caliber**: fkv sidgängnummer = `min(readers, --read-max-concurrency)`, standard
  Den övre gränsen är 4 (i linje med utvärderingsmaskinens vCPU), **inte** "en tråd per läsare" - den överskridna nivån ökar bara kön;
  Cache per tråd = 256 MiB / antal trådar (den totala mängden är fast, inte strimlad med läsare) och används
  `FuturesUnordered` Håll `--read-window` (standard 16) förfrågningar i farten. RocksDB sida är
  `readers` Varje `std::thread` läser samtidigt; enkelgängad ögla på fjällsidan (bortsett från läsarens utrustning,
  Endast som referenskolumn). `--no-fkv-read-threads` kan degenerera tillbaka till "interfolierade N asynkrona kedjor på en enda körningstid".
  Det är rörledningsdjupet, inte samtidighet, och det är inte jämförbart mellan olika motorer.
- **len()**: Ingen av de tre motorerna har O(1)-räkningsgränssnitt. Denna rad är hela tabellens räknehastighet, inte O(1)-uppslag.