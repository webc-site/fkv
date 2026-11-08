# Perfformiad

`fkv_bench` Profwch ysgrifennu, darllen, a sganiwch lwythi ar fwlch bysell sefydlog.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Mae'r dangosyddion yn cynnwys trwygyrch, hwyrni ar bob cam, a RSS brig. rhyddhau llif gwaith rhyddhau x86_64 /
`fkv_bench` deuaidd ar gyfer aarch64.

## Paru tair ffordd gyda RocksDB/fjall

`crate/perf/fkv_compare` Defnyddiwch yr un gyrrwr llwyfan a'r un amgodiad bysell (tanysgrifiad mawr-endian 8-beit ar y diwedd,
Mae trefn y geiriadur yn gyson â threfn y tanysgrifiad) Test fkv, RocksDB, fjall: RocksDB passed
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Mae'r llyfrgell a rennir a luniwyd wedi'i llwytho â dlopen (yr un C
Nid yw ABI, Rust build yn cysylltu â RocksDB), mae fjall yn defnyddio `fjall 3` yn uniongyrchol. Mae pob un o'r tair injan yn agor llyfrgell.
Symud ymlaen ar yr un llyfrgell yn nhrefn camau: llwyth swmp yn creu data, ac yna darlleniad len / pwynt / darlleniad ystod /
Mae dileu i gyd yn effeithio ar yr un data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Hepgorwch y drydedd golofn os nad oes RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Dim ond o un rhediad y gellir ei olrhain y gall y niferoedd ar gyfer y matrics hafan ddod:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI rhedeg <id>"
```

Mae `perf.yml` GitHub Actions yn gwneud hyn yn union (gosod `librocksdb-dev`, addaswyr rhaglen, rhedeg
Matrics rhes 25M, lanlwytho arteffact, ac ôl-lenwi'r dudalen yn uniongyrchol pan `backfill=true`).

### calibr

- ** Cymhareb Data/Cache**: 25,000,000 rhesi × 116 B ≈ 2766 MiB o ddata, 256 MiB o storfa fesul injan,
  ≈ 11: 1, wedi'i alinio ag adolygiad swyddogol RocksDB o ddata 97 GiB / storfa 6 GiB (≈ 16: 1).
- **Yr un faint o storfa**: celc tudalen fkv (yr un gronfa mynegai/data), storfa bloc RocksDB
  (`cache_index_and_filter_blocks=true`, maint bloc 4 KiB), storfa bloc fjall ei hun
  Mae'r cyllidebau i gyd yn cael eu gosod i'r un nifer o beit.
- **Caliber dyfalbarhad (mae'r golofn gymhariaeth yn defnyddio cyfluniad rhagosodedig RocksDB, heb aliniad)**: fkv 1 × tudalen data + fesul swp
  1 × maniffest `fdatasync` (Isomorffig i SyncData + AppendManifest o C++ EloqStore,
  Nid oes gan y naill na'r llall switsh i ffwrdd); mae colofn RocksDB yn defnyddio'r ffurfweddiad diofyn** (WAL ymlaen, `WriteOptions.sync=false`,
  Dim fsync dilyniannol), mae fjall hefyd yn defnyddio'r rhagosodiad (`PersistMode::Buffer`). Felly mae'r cyfnod ysgrifennu (ysgrifen sengl /
  Y gwahaniaeth rhwng ysgrifennu swp) yw'r **gwahaniaeth cryfder dyfalbarhad**, nid y gwahaniaeth gweithredu, y mae'n rhaid ei nodi ar y dudalen ynghyd â'r rhif.
  Os ydych chi eisiau dadansoddi "pwy sy'n gyflymach o dan yr un cryfder cydamseru", ychwanegwch `--align-fsync` (agor RocksDB gyda sync=true,
  fjall yn defnyddio SyncAll), ni ddefnyddir yr opsiwn hwn ar gyfer matricsau a gyhoeddir yn allanol.
- **Dau yrrwr fkv**: `--fkv-driver store` (diofyn, wedi'u cysylltu'n uniongyrchol ag un `Store` o fewn y broses, a
  Mae RocksDB / fjall hefyd yn alwadau llyfrgell) a `--fkv-driver runtime` (modd lleoli: proses
  `--fkv-shards` edafedd shard, mae ceisiadau'n cael eu cyfeirio gan allwedd, wedi'u halinio â darllen ac ysgrifennu cyhoeddus C++ EloqStore
  rhyngwyneb). Mae angen i bob cais gan yr olaf fynd trwy'r ciw + deffro traws-edau, brodorol (macOS, mae ffeil IO yn mynd trwy'r pwll edau)
  Mae amser darllen tua 20 µs/amser, sy'n drefn maint yn arafach na chysylltiad uniongyrchol yn y broses; llawer rhatach ar Linux/io_uring, ond
  Dal yn naid. Mae'r matrics tudalen yn defnyddio caliber y siop, a defnyddir y safon amser rhedeg ar gyfer "ffurflen defnyddio yn erbyn C++
  Cymhariaeth o "Rhyngwyneb Cyhoeddus" (darlleniad un edau un amser rhedeg fkv tua 50 kops/s, C++ EloqStore FFI edau sengl
  Shard tua 21 kops yr eiliad).
- **Darllen caliber arian cyfred**: rhif edefyn ochr fkv = `min(readers, --read-max-concurrency)`, rhagosodedig
  Y terfyn uchaf yw 4 (wedi'i alinio â'r peiriant gwerthuso vCPU), ** nid ** "un edefyn fesul darllenydd" - dim ond y ciw y mae'r lefel y rhagorwyd arno yn cynyddu;
  Cache fesul edefyn = 256 MiB / nifer yr edafedd (mae'r cyfanswm yn sefydlog, heb ei rwygo â darllenwyr), ac yn cael ei ddefnyddio
  `FuturesUnordered` Cadwch `--read-window` (diofyn 16) ceisiadau ar y hedfan. Ochr RocksDB yn
  `readers` Mae pob `std::thread` yn darllen ar yr un pryd; dolen un edau ar ochr fjall (gan anwybyddu gêr darllenwyr,
  Fel colofn gyfeirio yn unig). Gall `--no-fkv-read-threads` ddirywio yn ôl i "gadwyni asyncronaidd N rhyngddalennog ar un amser rhedeg".
  Dyna ddyfnder y biblinell, nid arian cyfred, ac nid oes modd ei gymharu ar draws peiriannau.
- **len()**: Nid oes gan yr un o'r tair injan ryngwyneb cyfrif O(1). Cyfradd cyfrif tabl llawn yw'r llinell hon, nid am-edrych O(1).