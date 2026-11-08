# Veiktspēja

`fkv_bench` Pārbaudiet rakstīšanas, lasīšanas un skenēšanas ielādes fiksētā atslēgas vietā.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatori ietver caurlaidspēju, latentumu katrā posmā un maksimālo RSS. laidienu darbplūsmas izlaidums x86_64 /
`fkv_bench` binārs aarch64.

## Trīsvirzienu spēle ar RocksDB/fjall

`crate/perf/fkv_compare` Izmantojiet to pašu posma draiveri un to pašu atslēgas kodējumu (8 baitu lielais apakšindekss beigās,
Vārdnīcas secība atbilst apakšindeksa secībai) Pārbaudīt fkv, RocksDB, fjall: RocksDB ir izturēts
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Kompilētā koplietotā bibliotēka tiek ielādēta ar dlopen (tā pati C
ABI, Rust build nesaista ar RocksDB), fjall izmanto `fjall 3` tieši. Katrs no trim dzinējiem atver bibliotēku.
Virzieties uz vienu un to pašu bibliotēku posmu secībā: lielapjoma ielāde izveido datus un pēc tam len / punkta nolasīšana / diapazona nolasīšana /
Visas dzēšanas ietekmē vienus un tos pašus datus.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Izlaist trešo kolonnu, ja nav RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Mājaslapas matricas skaitļus var iegūt tikai no vienas izsekojamās darbības:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --avots "CI palaist <id>"
```

GitHub Actions `perf.yml` dara tieši to (instalēt `librocksdb-dev`, programmēt adapterus, palaist
25 miljoni rindu matrica, augšupielādējiet artefaktu un tieši aizpildiet lapu, kad `backfill=true`).

### kalibrs

- **Datu/kešatmiņas attiecība**: 25 000 000 rindu × 116 B ≈ 2766 MiB datu, 256 MiB kešatmiņas vienam dzinējam,
  ≈ 11:1, saskaņots ar RocksDB oficiālo pārskatu par 97 GiB datiem/6 GiB kešatmiņu (≈ 16:1).
- **Tāds pats kešatmiņas apjoms**: fkv lapas kešatmiņa (indekss/datu kopums), RocksDB bloka kešatmiņa
  (`cache_index_and_filter_blocks=true`, bloka izmērs 4 KiB), paša fjall bloka kešatmiņa
  Visi budžeti ir iestatīti uz vienādu baitu skaitu.
- **Noturības kalibrs (salīdzinājuma slejā tiek izmantota RocksDB noklusējuma konfigurācija bez izlīdzināšanas)**: fkv 1 × datu lapa + par partiju
  1×manifests `fdatasync` (izomorfs SyncData + C++ EloqStore AppendManifest,
  Nevienam no tiem nav izslēgšanas slēdža); kolonnā RocksDB tiek izmantota **noklusējuma konfigurācija** (WAL ieslēgta, `WriteOptions.sync=false`,
  Nav secīgas fsync), fjall izmanto arī noklusējuma vērtību (`PersistMode::Buffer`). Tāpēc rakstīšanas fāze (viena rakstīšana /
  Atšķirība starp pakešu rakstīšanu) ir **noturības stipruma atšķirība**, nevis ieviešanas atšķirība, kas jānorāda lapā kopā ar numuru.
  Ja vēlaties analizēt "kurš ir ātrāks ar tādu pašu sinhronizācijas stiprumu", pievienojiet `--align-fsync` (atveriet RocksDB ar sync=true,
  fjall izmanto SyncAll), šī opcija netiek izmantota ārēji publicētām matricām.
- **Divi fkv draiveri**: `--fkv-driver store` (noklusējums, procesa laikā tieši savienots ar vienu `Store` un
  RocksDB/fjall ir arī bibliotēkas izsaukumi) un `--fkv-driver runtime` (izvietošanas režīms: process
  `--fkv-shards` shard pavedieni, pieprasījumi tiek maršrutēti pēc atslēgas, saskaņoti ar C++ EloqStore publisko lasīšanu un rakstīšanu
  interfeiss). Katram pēdējā pieprasījumam ir jāiet cauri rindai + starppavedienu modināšana, vietējais (macOS, fails IO iet caur pavedienu kopu)
  Lasīšanas laiks ir aptuveni 20 µs/laikā, kas ir par lielumu lēnāks nekā procesa tiešais savienojums; daudz lētāk uz Linux/io_uring, bet
  Joprojām lēciens. Lapas matricā tiek izmantots veikala kalibrs, un izpildlaika kalibrs tiek izmantots "izvietošanas formai pret C++".
  "Publiskās saskarnes" salīdzinājums (fkv izpildlaika viena pavediena nolasījums ir aptuveni 50 kops/s, C++ EloqStore FFI viena pavediena
  lauskas ap 21 kops/s).
- **Nolasīt vienlaicīguma kalibru**: fkv sānu vītnes numurs = `min(readers, --read-max-concurrency)`, noklusējuma
  Augšējā robeža ir 4 (saskaņota ar vērtēšanas mašīnu vCPU), **nevis** "viens pavediens uz lasītāju" - pārsniegtais līmenis tikai palielina rindu;
  Kešatmiņa vienam pavedienam = 256 MiB / pavedienu skaits (kopējā summa ir fiksēta, nevis sasmalcināta ar lasītājiem) un izmantota
  `FuturesUnordered` Saglabājiet `--read-window` (noklusējuma 16) pieprasījumus. RocksDB puse ir
  `readers` Katrs `std::thread` tiek lasīts vienlaicīgi; viena vītnes cilpa fjall pusē (ignorējot lasītāja rīkus,
  Tikai kā atsauces kolonnu). `--no-fkv-read-threads` var deģenerēties atpakaļ uz "savienotām N asinhronām ķēdēm vienā izpildlaikā".
  Tas ir cauruļvada dziļums, nevis vienlaicība, un tas nav salīdzināms starp dzinējiem.
- **len()**: nevienam no trim dzinējiem nav O(1) skaitīšanas saskarnes. Šī rinda ir pilna tabulas skaitīšanas ātrums, nevis O(1) uzmeklēšana.