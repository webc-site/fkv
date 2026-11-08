# Prestazzjoni

`fkv_bench` Ittestja t-tagħbijiet tal-kitba, il-qari u l-iskennjar fuq spazju fiss taċ-ċavetta.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

L-indikaturi jinkludu throughput, latency f'kull stadju, u l-ogħla RSS. rilaxx tal-fluss tax-xogħol rilaxx x86_64 /
`fkv_bench` binarju għal aarch64.

## Tlaqqigħ bi tliet direzzjonijiet ma' RocksDB/fjall

`crate/perf/fkv_compare` Uża l-istess sewwieq tal-istadju u l-istess kodifikazzjoni taċ-ċavetta (subscript big-endian ta’ 8 byte fl-aħħar,
L-ordni tad-dizzjunarju hija konsistenti mal-ordni tas-sottoskritt) Test fkv, RocksDB, fjall: RocksDB għadda
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Il-librerija kondiviża kkompilata hija mgħobbija b'dlopen (l-istess C
ABI, Rust build ma torbotx ma' RocksDB), fjall juża `fjall 3` direttament. Kull waħda mit-tliet magni tiftaħ librerija.
Avvanza fuq l-istess librerija fl-ordni tal-istadji: tagħbija bl-ingrossa toħloq data, u mbagħad len / qari tal-punt / qari tal-firxa /
It-tħassir kollu jaffettwa l-istess data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Aqbeż it-tielet kolonna jekk ma jkunx hemm RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

In-numri għall-matriċi tal-homepage jistgħu jiġu biss minn ġirja waħda traċċabbli:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

`perf.yml` ta' GitHub Actions jagħmel eżattament dan (installa `librocksdb-dev`, program adapters, ħaddem
25M ringiela matriċi, upload artifact, u direttament backfill il-paġna meta `backfill=true`).

### kalibru

- **Proporzjon tad-Data/Cache**: 25,000,000 ringiela × 116 B ≈ 2766 MiB ta’ data, 256 MiB ta’ cache għal kull magna,
  ≈ 11:1, allinjat mar-reviżjoni uffiċjali ta’ RocksDB ta’ 97 GiB data/6 GiB cache (≈ 16:1).
- **L-istess ammont ta' cache**: il-cache tal-paġna ta' fkv (indiċi/data l-istess ġabra), il-cache tal-blokk ta' RocksDB
  (`cache_index_and_filter_blocks=true`, block size 4 KiB), il-blokk cache ta' fjall stess
  Il-baġits huma kollha stabbiliti għall-istess numru ta 'bytes.
- **Kalibru tal-persistenza (il-kolonna tat-tqabbil tuża l-konfigurazzjoni default RocksDB, mingħajr allinjament)**: fkv 1 × paġna tad-dejta + għal kull lott
  1×manifest `fdatasync` (Iżomorfiku għal SyncData + AppendManifest ta' C++ EloqStore,
  Lanqas m'għandu swiċċ mitfi); il-kolonna RocksDB tuża l-**konfigurazzjoni default** (WAL fuq, `WriteOptions.sync=false`,
  L-ebda fsync sekwenzjali), fjall juża wkoll il-default (`PersistMode::Buffer`). Għalhekk il-fażi tal-kitba (kitba waħda /
  Id-differenza bejn il-kitbiet tal-lott) hija d-**differenza tas-saħħa tal-persistenza**, mhux id-differenza tal-implimentazzjoni, li trid tiġi ddikjarata fuq il-paġna flimkien man-numru.
  Jekk trid tanalizza "min hu aktar mgħaġġel taħt l-istess saħħa ta' sinkronizzazzjoni", żid `--align-fsync` (open RocksDB with sync=true,
  fjall juża SyncAll), din l-għażla ma tintużax għal matriċi ppubblikati esternament.
- **Żewġ sewwieqa fkv**: `--fkv-driver store` (default, konnessi direttament ma' `Store` wieħed fi ħdan il-proċess, u
  RocksDB / fjall huma wkoll sejħiet ta' librerija) u `--fkv-driver runtime` (mod ta' skjerament: proċess
  `--fkv-shards` shard threads, it-talbiet jintbagħtu biċ-ċavetta, allinjati mal-qari u l-kitba pubbliċi ta' C++ EloqStore
  interface). Kull talba ta 'dan ta' l-aħħar jeħtieġ li tgħaddi mill-kju + wake-up cross-thread, indiġeni (macOS, fajl IO jgħaddi mill-grupp ta 'ħajt)
  Il-ħin tal-qari huwa ta' madwar 20 µs/ħin, li huwa ordni ta' kobor aktar bil-mod minn konnessjoni diretta waqt il-proċess; ferm irħas fuq Linux/io_uring, iżda
  Għadu qabża. Il-matriċi tal-paġna tuża l-kalibru tal-maħżen, u l-kalibru tar-runtime jintuża għal "forma ta' skjerament vs C++
  Tqabbil ta' "Interface Pubblika" (fkv runtime single-thread qari huwa madwar 50 kops/s, C++ EloqStore FFI single-thread
  shard madwar 21 kops/s).
- **Aqra l-kalibru tal-konkorrenza**: numru tal-ħajta tal-ġenb fkv = `min(readers, --read-max-concurrency)`, default
  Il-limitu ta 'fuq huwa 4 (allinjat mal-magna ta' evalwazzjoni vCPU), **mhux** "ħajt wieħed għal kull qarrej" - il-livell maqbuż iżid biss il-kju;
  Cache għal kull ħajt = 256 MiB / numru ta 'ħjut (l-ammont totali huwa fiss, mhux imqatta' mal-qarrejja), u użat
  `FuturesUnordered` Żomm `--read-window` (default 16) talbiet fuq il-fly. In-naħa RocksDB hija
  `readers` Kull `std::thread` jaqra fl-istess ħin; loop b'ħajt wieħed fuq in-naħa tal-fjall (jinjora l-irkaptu tal-qarrejja,
  Bħala kolonna ta’ referenza biss). `--no-fkv-read-threads` jista' jiddeġenera lura għal "ktajjen N asinkroniċi interleaved fuq runtime wieħed".
  Dak huwa l-fond tal-pipeline, mhux il-konkorrenza, u mhux komparabbli bejn il-magni.
- **len()**: L-ebda waħda mit-tliet magni ma għandha interfaces tal-għadd O(1). Din il-linja hija r-rata tal-għadd tal-mejda sħiħa, mhux O(1) lookup.