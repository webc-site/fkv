# Errendimendua

`fkv_bench` Probatu idazketa, irakurketa eta eskaneatu kargak gako-espazio finko batean.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Adierazleen artean fluxua, latentzia etapa bakoitzean eta RSS gailurra daude. askatu lan-fluxuaren oharra x86_64 /
`fkv_bench` aarch64rako bitarra.

## Hiruko partida RocksDB/fjall-ekin

`crate/perf/fkv_compare` Erabili etapa-gidari bera eta gako-kodeketa bera (8 byte big-endian azpiindizea amaieran,
Hiztegi-ordena azpiindize-ordenarekin bat dator) Test fkv, RocksDB, fjall: RocksDB gainditu
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Konpilatutako liburutegi partekatua dlopen-ekin kargatzen da (C
ABI, Rust eraikitzeak ez du RocksDB-rekin lotzen), fjall-ek `fjall 3` zuzenean erabiltzen du. Hiru motor bakoitzak liburutegi bat irekitzen du.
Liburutegi berean aurrera egin faseen ordenan: kargak datuak sortzen ditu, eta gero len / puntu irakurketa / barruti irakurketa /
Ezabatzeak datu berdinei eragiten die.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Saltatu hirugarren zutabea RocksDB ez badago
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Hasierako orriaren matrizearen zenbakiak trazagarritasun bakarretik etor daitezke:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions-en `perf.yml`k hauxe egiten du (instalatu `librocksdb-dev`, programatu egokitzaileak, exekutatu
25 M errenkada-matrizea, kargatu artefaktua eta zuzenean bete orria `backfill=true` denean).

### kalibrea

- **Datuak/Cache ratioa**: 25.000.000 errenkada × 116 B ≈ 2766 MiB datu, 256 MiB cache motor bakoitzeko,
  ≈ 11:1, RocksDB-k 97 GiB datu/6 GiB cache-ren berrikuspen ofizialarekin bat eginda (≈ 16:1).
- **Cache kopuru bera**: fkv-ren orrialde-cachea (indizea/datuen multzoa bera), RocksDB-ren bloke-cachea
  (`cache_index_and_filter_blocks=true`, blokearen tamaina 4 KiB), fjall-en bloke-cachea
  Aurrekontuak byte kopuru berdinean ezartzen dira.
- **Iraunkortasun kalibrea (konparazio zutabeak RocksDB konfigurazio lehenetsia erabiltzen du, lerrokatu gabe)**: fkv 1 × datu orria + lote bakoitzeko
  1×manifestazioa `fdatasync` (isomorfoa SyncData + C++ EloqStore-ren AppendManifest,
  Ez du etengailurik ere); RocksDB zutabeak **konfigurazio lehenetsia** erabiltzen du (WAL aktibatuta, `WriteOptions.sync=false`,
  Ez dago fsync sekuentziala), fjall-ek lehenetsia ere erabiltzen du (`PersistMode::Buffer`). Beraz, idazketa fasea (idazketa bakarra /
  Batch idazketen arteko aldea) **iraunkortasun-indarraren aldea** da, ez inplementazio-aldea, eta orrialdean adierazi behar da zenbakiarekin batera.
  "Nor den azkarragoa sinkronizazio indar berdinarekin" aztertu nahi baduzu, gehitu `--align-fsync` (ireki RocksDB sync=true-rekin,
  fjall-ek SyncAll erabiltzen du), aukera hau ez da kanpoan argitaratutako matrizeetarako erabiltzen.
- **Bi fkv kontrolatzaile**: `--fkv-driver store` (lehenetsia, prozesu barruan `Store` bakar batera zuzenean konektatuta, eta
  RocksDB / fjall liburutegi-deiak ere badira) eta `--fkv-driver runtime` (inplementazio modua: prozesu bat
  `--fkv-shards` zati-hariak, eskaerak gakoen bidez bideratzen dira, C++ EloqStore-ren irakurketa eta idazketa publikoarekin lerrokatuta.
  interfazea). Azken honen eskaera bakoitzak ilaratik pasa behar du + hari gurutzatua esnatzea, jatorrizkoa (macOS, IO fitxategia hari multzotik pasatzen da)
  Irakurtzeko denbora 20 µs/denbora ingurukoa da, hau da, prozesuko konexio zuzena baino magnitude ordena bat motelagoa; askoz merkeagoa Linux/io_uring-en, baina
  Oraindik salto bat. Orrialde matrizeak denda kalibrea erabiltzen du, eta exekuzio kalibrea "inplementazio formularioa vs C++"-rako erabiltzen da.
  "Interfaze Publikoaren" alderaketa (fkv runtime hari bakarreko irakurketa 50 kops/s ingurukoa da, C++ EloqStore FFI hari bakarrekoa
  zatia 21 kop/s).
- **Irakurri aldibereko kalibrea**: fkv alboko hariaren zenbakia = `min(readers, --read-max-concurrency)`, lehenetsia
  Goiko muga 4 da (ebaluazio-makinaren vCPUarekin lerrokatuta), **ez** "irakurle bakoitzeko hari bat" - gainditutako mailak ilara handitzen du soilik;
  Hari bakoitzeko cachea = 256 MiB / hari kopurua (kopuru osoa finkoa da, ez da irakurleekin txikitu) eta erabiltzen
  `FuturesUnordered` Mantendu `--read-window` (lehenetsia 16) eskaerak berehala. RocksDB aldea da
  `readers` `std::thread` bakoitzak aldi berean irakurtzen du; hari bakarreko begizta fjall aldean (irakurleen engranajea alde batera utzita,
  Erreferentzia-zutabe gisa soilik). `--no-fkv-read-threads` "N kate asinkrono tartekatuak exekuzio-denbora bakarrean" itzul daitezke.
  Hori hodiaren sakonera da, ez aldiberekotasuna, eta ez da konparagarria motorretan.
- **len()**: hiru motorrek ez dute O(1) zenbaketa interfazerik. Lerro hau taula osoko zenbaketa-tasa da, ez O(1) bilaketa.