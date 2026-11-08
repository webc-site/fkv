# Werkverrigting

`fkv_bench` Toetsskryf-, lees- en skandeerladings op 'n vaste sleutelspasie.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Aanwysers sluit in deurset, latency in elke stadium, en piek RSS. vrystelling werkvloei vrystelling x86_64 /
`fkv_bench` binêr vir aarch64.

## Drie-rigting passing met RocksDB/fjall

`crate/perf/fkv_compare` Gebruik dieselfde verhoogbestuurder en dieselfde sleutelkodering (8-grepe groot-endian subskripsie aan die einde,
Woordeboekvolgorde stem ooreen met onderskrifvolgorde) Toets fkv, RocksDB, fjall: RocksDB geslaag
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Die saamgestelde gedeelde biblioteek is gelaai met dlopen (dieselfde C
ABI, Rust build skakel nie na RocksDB nie), fjall gebruik `fjall 3` direk. Elkeen van die drie enjins maak 'n biblioteek oop.
Gaan voort op dieselfde biblioteek in die volgorde van stadiums: grootmaatlading skep data, en dan len / puntlesing / reekslesing /
Uitvee raak almal dieselfde data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Slaan die derde kolom oor as daar geen RocksDB is nie
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Die nommers vir die tuisbladmatriks kan slegs van een naspeurbare lopie kom:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --masjien "..." --bron "CI run <id>"
```

GitHub Actions se `perf.yml` doen presies dit (installeer `librocksdb-dev`, programadapters, hardloop
25M rymatriks, laai artefak op en vul die bladsy direk terug wanneer `backfill=true`).

### kaliber

- **Data/kasverhouding**: 25 000 000 rye × 116 B ≈ 2766 MiB data, 256 MiB kas per enjin,
  ≈ 11:1, in lyn met RocksDB se amptelike hersiening van 97 GiB-data/6 GiB-kas (≈ 16:1).
- **Dieselfde hoeveelheid kas**: fkv se bladsykas (indeks/data dieselfde poel), RocksDB se blokkas
  (`cache_index_and_filter_blocks=true`, blokgrootte 4 KiB), fjall se eie blokkas
  Die begrotings is almal op dieselfde aantal grepe gestel.
- **Volhardingskaliber (die vergelykingskolom gebruik die RocksDB verstekkonfigurasie, sonder belyning)**: fkv 1×databladsy + per bondel
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Nie een het 'n afskakelaar nie); die RocksDB-kolom gebruik die **verstekkonfigurasie** (WAL aan, `WriteOptions.sync=false`,
  Geen opeenvolgende fsync nie), fjall gebruik ook die verstek (`PersistMode::Buffer`). Daarom is die skryffase (enkelskryf /
  Die verskil tussen bondelskryf) is die **volhardingsterkteverskil**, nie die implementeringsverskil nie, wat saam met die nommer op die bladsy gemeld moet word.
  As jy wil ontleed "wie is vinniger onder dieselfde sinchronisasiesterkte", voeg `--align-fsync` by (maak RocksDB oop met sync=true,
  fjall gebruik SyncAll), word hierdie opsie nie vir ekstern gepubliseerde matrikse gebruik nie.
- **Twee fkv-bestuurders**: `--fkv-driver store` (verstek, direk gekoppel aan 'n enkele `Store` binne die proses, en
  RocksDB / fjall is ook biblioteekoproepe) en `--fkv-driver runtime` (ontplooiingsmodus: 'n proses
  `--fkv-shards` skerfdrade, versoeke word deur sleutel gestuur, in lyn met publieke lees en skryf van C++ EloqStore
  koppelvlak). Elke versoek van laasgenoemde moet deur die tou gaan + kruisdraadopwekking, inheems (macOS, lêer IO gaan deur die draadpoel)
  Leestyd is ongeveer 20 µs/tyd, wat 'n orde van grootte stadiger is as in-proses direkte verbinding; baie goedkoper op Linux/io_uring, maar
  Steeds 'n sprong. Die bladsymatriks gebruik die winkelkaliber, en die looptydkaliber word gebruik vir "ontplooiingsvorm vs C++
  Vergelyking van "Publieke Interface" (fkv runtime enkeldraadlesing is ongeveer 50 kops/s, C++ EloqStore FFI enkeldraad
  skerf ongeveer 21 kops/s).
- **Lees gelyktydige kaliber**: fkv sydraadnommer = `min(readers, --read-max-concurrency)`, verstek
  Die boonste limiet is 4 (belyn met die evalueringsmasjien vCPU), **nie** "een draad per leser" - die oorskryde vlak verhoog net die tou;
  Kas per draad = 256 MiB / aantal drade (die totale bedrag is vas, nie versnipper met lesers nie), en gebruik
  `FuturesUnordered` Hou `--read-window` (verstek 16) versoeke aan die gang. RocksDB kant is
  `readers` Elke `std::thread` lees gelyktydig; enkel-draad lus aan fjall kant (ignoreer leserstoerusting,
  Slegs as 'n verwysingskolom). `--no-fkv-read-threads` kan terugdegenereer na "ingevoegde N asynchrone kettings op 'n enkele looptyd".
  Dit is die pyplyndiepte, nie gelyktydig nie, en is nie vergelykbaar oor enjins nie.
- **len()**: Nie een van die drie enjins het O(1)-tel-koppelvlakke nie. Hierdie lyn is die volle tabeltelkoers, nie O(1)-opsoek nie.