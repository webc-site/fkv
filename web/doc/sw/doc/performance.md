# Utendaji

`fkv_bench` Jaribu kuandika, kusoma na kuchanganua mizigo kwenye nafasi ya ufunguo usiobadilika.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Viashirio ni pamoja na matokeo, muda wa kusubiri katika kila hatua, na kilele cha RSS. toa toleo la mtiririko wa kazi x86_64 /
`fkv_bench` binary kwa aarch64.

## Ulinganifu wa njia tatu na RocksDB/fjall

`crate/perf/fkv_compare` Tumia kiendesha hatua sawa na usimbaji wa vitufe sawa (8-byte big-endian subscript mwishoni,
Mpangilio wa kamusi unaendana na mpangilio wa hati ndogo) Jaribio la fkv, RocksDB, fjall: RocksDB imefaulu
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Maktaba iliyoshirikiwa iliyokusanywa imepakiwa na dlopen (C
ABI, Rust build haiunganishi na RocksDB), fjall hutumia `fjall 3` moja kwa moja. Kila moja ya injini tatu hufungua maktaba.
Kuendeleza maktaba hiyo hiyo kwa mpangilio wa hatua: mzigo wa wingi huunda data, na kisha kusoma kwa len / uhakika / usomaji wa anuwai /
Ufutaji wote huathiri data sawa.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Ruka safu wima ya tatu ikiwa hakuna RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Nambari za matrix ya ukurasa wa nyumbani zinaweza tu kutoka kwa kukimbia moja inayoweza kufuatiliwa:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --mashine "..." --source "CI endesha <id>"
```

GitHub Actions' `perf.yml` hufanya hivi hasa (sakinisha `librocksdb-dev`, adapta za programu, endesha
Matrix ya safu mlalo ya 25M, pakia vizalia vya programu, na ujaze ukurasa moja kwa moja wakati `backfill=true`).

### caliber

- **Uwiano wa Data/Cache**: safu mlalo 25,000,000 × 116 B ≈ MiB 2766 ya data, MiB 256 ya akiba kwa kila injini,
  ≈ 11:1, iliyoambatanishwa na ukaguzi rasmi wa RocksDB wa data ya GiB 97/kache 6 ya GiB (≈ 16:1).
- ** Kiasi sawa cha kashe **: kashe ya ukurasa wa fkv (index/data sawa), kashe ya kuzuia ya RocksDB
  (`cache_index_and_filter_blocks=true`, zuia ukubwa wa 4 KiB), akiba ya kizuizi cha fjall mwenyewe
  Bajeti zote zimewekwa kwa idadi sawa ya baiti.
- **Uwezo wa kudumu (safu ya kulinganisha hutumia usanidi chaguo-msingi wa RocksDB, bila upatanishi)**: fkv 1× ukurasa wa data + kwa kila kundi
  1×manifest `fdatasync` (Isomorphic kwa SyncData + AppendManifest ya C++ EloqStore,
  Wala hana swichi ya kuzima); safu wima ya RocksDB hutumia **usanidi chaguo-msingi** (WAL on, `WriteOptions.sync=false`,
  Hakuna fsync mfuatano), fjall pia hutumia chaguo-msingi (`PersistMode::Buffer`). Kwa hivyo awamu ya uandishi (andika moja /
  Tofauti kati ya maandishi ya kundi) ni **tofauti ya nguvu ya kudumu**, sio tofauti ya utekelezaji, ambayo lazima ielezwe kwenye ukurasa pamoja na nambari.
  Ikiwa unataka kuchanganua "nani ana kasi chini ya nguvu sawa ya ulandanishi", ongeza `--align-fsync` (fungua RocksDB na sync=true,
  fjall inatumia SyncAll), chaguo hili halitumiki kwa matrices zilizochapishwa nje.
- **Viendeshi viwili vya fkv**: `--fkv-driver store` (chaguo-msingi, iliyounganishwa moja kwa moja kwa `Store` moja ndani ya mchakato, na
  RocksDB / fjall pia ni simu za maktaba) na `--fkv-driver runtime` (hali ya kupeleka: mchakato
  `--fkv-shards` Kida wa shati 线程, ombi limeelekezwa kwa key, linganisha na usomaji na uandishi wa umma wa C++ EloqStore
  interface). Kila ombi la mwisho linahitaji kupitia foleni + kuamka kwa nyuzi, asili (macOS, faili IO inapitia dimbwi la nyuzi)
  Muda wa kusoma ni takriban µs 20/saa, ambayo ni mpangilio wa ukubwa wa polepole kuliko muunganisho wa moja kwa moja wa mchakato; nafuu zaidi kwenye Linux/io_uring, lakini
  Bado kuruka. Matrix ya ukurasa hutumia caliber ya duka, na caliber ya wakati wa utekelezaji inatumika kwa "fomu ya kupeleka dhidi ya C++
  Ulinganisho wa "Public Interface" (usomaji wa uzi mmoja wa fkv wakati wa kukimbia ni takriban 50 kops/s, C++ EloqStore FFI yenye uzi mmoja
  shard kuhusu 21 kops/s).
- **Soma kiwango cha sarafu**: nambari ya uzi wa upande wa fkv = `min(readers, --read-max-concurrency)`, chaguomsingi
  Upeo wa juu ni 4 (iliyounganishwa na mashine ya tathmini vCPU), ** si ** "thread moja kwa kila msomaji" - kiwango kilichozidi huongeza tu foleni;
  Cache kwa kila thread = 256 MiB / idadi ya nyuzi (jumla ya kiasi ni fasta, si kupasuliwa na wasomaji), na kutumika
  `FuturesUnordered` Weka `--read-window` maombi (chaguo-msingi 16) kwa haraka. RocksDB upande ni
  `readers` Kila `std::thread` inasoma kwa wakati mmoja; kitanzi chenye nyuzi moja upande wa fjall (kupuuza gia ya wasomaji,
  Kama safu ya kumbukumbu tu). `--no-fkv-read-threads` inaweza kurudi nyuma hadi "minyororo N iliyoingiliana isiyolingana kwa wakati mmoja wa utekelezaji".
  Huo ni kina cha bomba, sio kuunganishwa, na hauwezi kulinganishwa na injini zote.
- **len()**: Hakuna injini yoyote kati ya hizo tatu iliyo na violesura vya O(1) vya kuhesabu. Mstari huu ndio kiwango kamili cha kuhesabu jedwali, sio utafutaji wa O(1).