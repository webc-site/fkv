# Afköst

`fkv_bench` Prófaðu að skrifa, lesa og skanna álag á föstu lyklarými.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Vísar innihalda afköst, leynd á hverju stigi og hámarks RSS. gefa út verkflæðisútgáfu x86_64 /
`fkv_bench` tvöfaldur fyrir aarch64.

## Þríhliða samsvörun við RocksDB/fjall

`crate/perf/fkv_compare` Notaðu sama sviðsdrif og sömu lykilkóðun (8 bæta stór-endian áskrift í lokin,
Orðabókaröð er í samræmi við undirskriftarröð) Próf fkv, RocksDB, fjall: RocksDB samþykkt
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Samnýtta samnýtta bókasafnið er hlaðið með dlopen (sama C
ABI, Rust build tengist ekki RocksDB), fjall notar `fjall 3` beint. Hver vélanna þriggja opnar bókasafn.
Farðu áfram á sama bókasafni í röð áföngum: magnhleðsla býr til gögn, og síðan len / punktalestur / billestur /
Eyðingar hafa allar áhrif á sömu gögnin.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Slepptu þriðja dálknum ef það er engin RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Tölurnar fyrir heimasíðu fylkið geta aðeins komið frá einni rekjanlegri keyrslu:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --vél "..." --heimild "CI run <id>"
```

GitHub Actions' `perf.yml` gerir nákvæmlega þetta (settu upp `librocksdb-dev`, forritaðu millistykki, keyrðu
25M raðfylki, hlaðið upp gripi og fylltu beint út á síðuna þegar `backfill=true`).

### kaliber

- **Gögn/skyndiminni hlutfall**: 25.000.000 línur × 116 B ≈ 2766 MiB af gögnum, 256 MiB af skyndiminni á hverja vél,
  ≈ 11:1, í takt við opinbera endurskoðun RocksDB á 97 GiB gögnum/6 GiB skyndiminni (≈ 16:1).
- **Sama magn af skyndiminni**: síðu skyndiminni fkv (vísitölu/gögn sama hóp), blokk skyndiminni RocksDB
  (`cache_index_and_filter_blocks=true`, blokkastærð 4 KiB), eigin blokkageymslu fjalls
  Fjárveitingar eru allar stilltar á sama fjölda bæta.
- ** Þrautseigju kaliber (samanburðardálkurinn notar RocksDB sjálfgefna stillingu, án jöfnunar)**: fkv 1×gagnasíða + per lotu
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Hvorugur hefur slökkt rofa); RocksDB dálkurinn notar **sjálfgefnar stillingar** (WAL on, `WriteOptions.sync=false`,
  Enginn sequential fsync), fjall notar einnig sjálfgefið (`PersistMode::Buffer`). Þess vegna skrifa fasinn (einn skrifa /
  Munurinn á lotuskrifum) er **þolþolsmunurinn**, ekki útfærslumunurinn, sem þarf að koma fram á síðunni ásamt númerinu.
  Ef þú vilt greina „hver er fljótari undir sama samstillingarstyrk“ skaltu bæta við `--align-fsync` (opnaðu RocksDB með sync=true,
  fjall notar SyncAll), þessi valkostur er ekki notaður fyrir útgefið fylki.
- **Tveir fkv ökumenn**: `--fkv-driver store` (sjálfgefið, beintengt við einn `Store` í ferlinu, og
  RocksDB / fjall eru líka bókasafnssímtöl) og `--fkv-driver runtime` (dreifingarhamur: ferli
  `--fkv-shards` sundurþræðir, beiðnir eru sendar með lykli, í takt við almennan lestur og ritun á C++ EloqStore
  viðmót). Sérhver beiðni þess síðarnefnda þarf að fara í gegnum biðröðina + þráðavöknun, innfædd (macOS, skrá IO fer í gegnum þráðasafnið)
  Lestrartími er um 20 µs/tíma, sem er stærðargráðu hægari en bein tenging í vinnslu; miklu ódýrara á Linux/io_uring, en
  Enn stökk. Síðufylkiið notar verslunarkaliberið og keyrslumagnið er notað fyrir "dreifingarform vs C++
  Samanburður á "Public Interface" (fkv keyrslutími eins þráðar lestur er um 50 kops/s, C++ EloqStore FFI einn þráður
  brot um 21 kops/s).
- **Lestu samhliða kaliber**: fkv hliðarþráðarnúmer = `min(readers, --read-max-concurrency)`, sjálfgefið
  Efri mörkin eru 4 (samræmd við matsvél vCPU), **ekki** "einn þráður á hvern lesanda" - það stig sem farið er yfir eykur aðeins biðröðina;
  Skyndiminni á þráð = 256 MiB / fjöldi þráða (heildarupphæðin er föst, ekki tætt með lesendum) og notuð
  `FuturesUnordered` Haltu `--read-window` (sjálfgefið 16) beiðnum á flugi. RocksDB hlið er
  `readers` Hver `std::thread` les samtímis; einþráða lykkja á fjallsmegin (að hundsað lesbúnað,
  Aðeins sem viðmiðunardálkur). `--no-fkv-read-threads` getur hrörnað aftur í "fléttaðar N ósamstilltar keðjur á einni keyrslutíma".
  Það er dýpt leiðslunnar, ekki samhliða, og er ekki sambærilegt milli véla.
- **len()**: Engin af vélunum þremur er með O(1) talningarviðmót. Þessi lína er heildartalningarhlutfall töflunnar, ekki O(1) uppflettingu.