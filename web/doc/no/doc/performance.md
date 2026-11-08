# Ytelse

`fkv_bench` Testskrive, lese og skanne laster på en fast nøkkelplass.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatorer inkluderer gjennomstrømning, latens på hvert trinn og topp RSS. frigjør arbeidsflytutgivelse x86_64 /
`fkv_bench` binær for aarch64.

## Treveis matchup med RocksDB/fjall

`crate/perf/fkv_compare` Bruk samme scenedriver og samme nøkkelkoding (8-byte big-endian subscript på slutten,
Ordbokrekkefølgen samsvarer med nedskrevet rekkefølge) Test fkv, RocksDB, fjall: RocksDB bestått
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Det kompilerte delte biblioteket er lastet med dlopen (samme C
ABI, Rust build kobler ikke til RocksDB), fjall bruker `fjall 3` direkte. Hver av de tre motorene åpner et bibliotek.
Gå videre på det samme biblioteket i rekkefølgen av trinn: massebelastning skaper data, og deretter len / punktlesing / rekkeviddelesing /
Alle slettinger påvirker de samme dataene.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Hopp over den tredje kolonnen hvis det ikke er noen RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Tallene for hjemmesidematrisen kan bare komme fra én sporbar kjøring:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --maskin "..." --kilde "CI run <id>"
```

GitHub Actions' `perf.yml` gjør akkurat dette (installer `librocksdb-dev`, programmer adaptere, kjør
25M radmatrise, last opp artefakter og fyll siden direkte når `backfill=true`).

### kaliber

- **Data/cache-forhold**: 25 000 000 rader × 116 B ≈ 2766 MiB data, 256 MiB cache per motor,
  ≈ 11:1, på linje med RocksDBs offisielle gjennomgang av 97 GiB data/6 GiB cache (≈ 16:1).
- **Samme mengde hurtigbuffer**: fkvs sidebuffer (indeks/data samme pool), RocksDBs blokkbuffer
  (`cache_index_and_filter_blocks=true`, blokkstørrelse 4 KiB), fjalls egen blokkcache
  Budsjettene er alle satt til samme antall byte.
- **Persistens kaliber (sammenligningskolonnen bruker RocksDB standardkonfigurasjon, uten justering)**: fkv 1×dataside + per batch
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Ingen av dem har en av-bryter); RocksDB-kolonnen bruker **standardkonfigurasjonen** (WAL på, `WriteOptions.sync=false`,
  Ingen sekvensiell fsync), fjall bruker også standarden (`PersistMode::Buffer`). Derfor skrivefasen (enkeltskriving /
  Forskjellen mellom batchskrivninger) er **persistensstyrkeforskjellen**, ikke implementeringsforskjellen, som må oppgis på siden sammen med nummeret.
  Hvis du vil analysere "hvem er raskere under samme synkroniseringsstyrke", legg til `--align-fsync` (åpne RocksDB med sync=true,
  fjall bruker SyncAll), dette alternativet brukes ikke for eksternt publiserte matriser.
- **To fkv-drivere**: `--fkv-driver store` (standard, direkte koblet til en enkelt `Store` i prosessen, og
  RocksDB / fjall er også bibliotekanrop) og `--fkv-driver runtime` (distribusjonsmodus: en prosess
  `--fkv-shards` shard tråder, forespørsler rutes med nøkkel, på linje med offentlig lesing og skriving av C++ EloqStore
  grensesnitt). Hver forespørsel fra sistnevnte må gå gjennom køen + tverrtrådsoppvåkning, native (macOS, fil IO går gjennom trådpoolen)
  Lesetiden er omtrent 20 µs/tid, som er en størrelsesorden langsommere enn direkte tilkobling under prosess; mye billigere på Linux/io_uring, men
  Fortsatt et hopp. Sidematrisen bruker butikkkaliberet, og kjøretidskaliberet brukes for "distribusjonsskjema vs C++
  Sammenligning av "Public Interface" (fkv kjøretid entrådsavlesning er omtrent 50 kops/s, C++ EloqStore FFI enkelttråd
  skår ca 21 kops/s).
- **Les samtidighetskaliber**: fkv sidetrådnummer = `min(readers, --read-max-concurrency)`, standard
  Den øvre grensen er 4 (justert med evalueringsmaskinens vCPU), **ikke** "én tråd per leser" - det overskridede nivået øker bare køen;
  Cache per tråd = 256 MiB / antall tråder (totalt beløp er fast, ikke makulert med lesere), og brukt
  `FuturesUnordered` Hold `--read-window` (standard 16) forespørsler på farten. RocksDB side er
  `readers` Hver `std::thread` leser samtidig; entrådet løkke på fjellsiden (ignorerer leserutstyr,
  Kun som referansekolonne). `--no-fkv-read-threads` kan degenerere tilbake til "sammenflettede N asynkrone kjeder på en enkelt kjøretid".
  Det er rørledningsdybden, ikke samtidighet, og er ikke sammenlignbar på tvers av motorer.
- **len()**: Ingen av de tre motorene har O(1)-tellegrensesnitt. Denne linjen er hele tabellens tellehastighet, ikke O(1)-oppslag.