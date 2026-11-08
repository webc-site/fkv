# Leeschtung

`fkv_bench` Test schreiwen, liesen a scannt Lasten op engem fixen Schlësselraum.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatoren enthalen Duerchgang, Latenz op all Etapp, a Peak RSS. Fräisetzung Workflow Verëffentlechung x86_64 /
`fkv_bench` binär fir aarch64.

## Dräi-Wee Matchup mat RocksDB / Fjäll

`crate/perf/fkv_compare` Benotzt deeselwechte Bühne-Treiber an déiselwecht Schlësselkodéierung (8-Byte Big-Endian Abonnement um Enn,
Wierderbuch Uerdnung ass konsequent mat Abonnement Uerdnung) Test fkv, RocksDB, fjall: RocksDB passéiert
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Déi kompiléiert gedeelt Bibliothéik gëtt mat dlopen gelueden (déi selwecht C
ABI, Rust Bau verbënnt net mat RocksDB), fjall benotzt `fjall 3` direkt. Jiddereng vun den dräi Motore mécht eng Bibliothéik op.
Fortschrëtt op déi selwecht Bibliothéik an der Reiefolleg vun Etappen: Massebelaaschtung erstellt Daten, an dann len / Punkt Liesen / Range Liesen /
Läschen beaflossen all déiselwecht Donnéeën.

```bash
cargo build --release -p fkv_compare
bash Scripts/perf/build_rocksdb_ffi.sh # Sprangen déi drëtt Kolonn wann et keng RocksDB gëtt
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

D'Zuelen fir d'Homepage Matrix kënnen nëmmen aus engem tracéierbare Laf kommen:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions '`perf.yml` mécht genau dat (installéieren `librocksdb-dev`, programméiere Adapter, lafen
25M Rei Matrix, Eroplueden Artefakt, an direkt d'Säit zréckfëllen wann `backfill=true`).

### kaliber

- **Daten/Cache Verhältnis**: 25.000.000 Reihen × 116 B ≈ 2766 MiB vun Daten, 256 MiB Cache pro Motor,
  ≈ 11: 1, ausgeriicht mam RocksDB offiziell Iwwerpréiwung vun 97 GiB Daten / 6 GiB Cache (≈ 16: 1).
- ** Selwecht Betrag vun Cache **: fkv Säit Cache (Index / Daten selwechte Pool), RocksDB's Block Cache
  (`cache_index_and_filter_blocks=true`, Blockgréisst 4 KiB), dem fjall säin eegene Blockcache
  D'Budgets sinn all op déiselwecht Zuel vu Bytes gesat.
- ** Persistenz Kaliber (d'Vergläichskolonne benotzt d'RocksDB Standardkonfiguratioun, ouni Ausriichtung) **: fkv 1 × Daten Säit + pro Batch
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Weder huet en Ausschalter); d'RocksDB Kolonn benotzt d'**Standardkonfiguratioun** (WAL op, `WriteOptions.sync=false`,
  Kee sequenziell fsync), fjall benotzt och den Default (`PersistMode::Buffer`). Dofir ass d'Schreifphase (eenzel Schreiwen /
  Den Ënnerscheed tëscht de Batchschreift) ass den **Persistenzstäerktdifferenz**, net den Implementatiounsdifferenz, deen op der Säit zesumme mat der Zuel muss uginn.
  Wann Dir wëllt analyséieren "wien méi séier ënner der selwechter Synchroniséierungsstäerkt ass", füügt `--align-fsync` derbäi (oppen RocksDB mat Sync=true,
  fjall benotzt SyncAll), gëtt dës Optioun net fir extern publizéiert Matrizen benotzt.
- **Zwee fkv Chauffeuren**: `--fkv-driver store` (Standard, direkt verbonne mat engem eenzegen `Store` am Prozess, an
  RocksDB / fjall sinn och Bibliothéiks-Uriff) an `--fkv-driver runtime` (Deployment-Modus: e Prozess
  `--fkv-shards` Shard Threads, Ufroe ginn duerch Schlëssel geréckelt, ausgeriicht mat ëffentlecher Liesen a Schreiwen vum C++ EloqStore
  Interface). All Ufro vun der leschter muss duerch d'Schlaang + Cross-Thread Wake-up goen, gebierteg (macOS, Datei IO geet duerch de thread Pool)
  D'Lieszäit ass ongeféier 20 µs / Zäit, wat eng Uerdnung vun der Gréisst méi lues ass wéi am Prozess direkt Verbindung; vill méi bëlleg op Linux / io_uring, mä
  Nach ëmmer e Sprong. D'Säitmatrix benotzt de Store Kaliber, an de Runtime Kaliber gëtt benotzt fir "Deployment Form vs C++
  Verglach vu "Public Interface" (fkv Runtime Single-Thread Liesung ass ongeféier 50 kops/s, C++ EloqStore FFI Single-thread
  Schärft ongeféier 21 kops / s).
- **Concurrency Kaliber liesen**: fkv Säit thread Zuel = `min(readers, --read-max-concurrency)`, Standard
  Déi iewescht Limit ass 4 (ausgeriicht mat der Evaluatiounsmaschinn vCPU), ** net ** "ee Fuedem pro Lieser" - den iwwerschratten Niveau erhéicht nëmmen d'Schlaang;
  Cache pro Fuedem = 256 MiB / Unzuel vun Threads (de Gesamtbetrag ass fixéiert, net mat Lieser zerklengert), a benotzt
  `FuturesUnordered` Halt `--read-window` (Standard 16) Ufroen op der Flucht. RocksDB Säit ass
  `readers` All `std::thread` liest gläichzäiteg; Single-threaded Loop op Fjäll Säit (Ignoréiere vum Lieserausrüstung,
  Nëmmen als Referenzkolonne). `--no-fkv-read-threads` kënnen zréck op "interleaved N asynchrone Ketten op enger eenzeger Runtime" degeneréieren.
  Dat ass Pipeline Déift, net concurrency, an ass net vergläichbar iwwer Motore.
- ** len () **: Keen vun den dräi Motore hunn O (1) zielen Schnëttplazen. Dës Zeil ass de ganzen Dëschzielungsquote, net O (1) Lookup.