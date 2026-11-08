# Rendiment

`fkv_bench` Proveu les càrregues d'escriptura, lectura i exploració en un espai de clau fixa.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Els indicadors inclouen el rendiment, la latència a cada etapa i l'RSS màxim. llançament del flux de treball x86_64 /
`fkv_bench` binari per a aarch64.

## Partit a tres bandes amb RocksDB/fjall

`crate/perf/fkv_compare` Utilitzeu el mateix controlador d'escenari i la mateixa codificació de claus (subíndex big-endian de 8 bytes al final,
L'ordre del diccionari és coherent amb l'ordre dels subíndexs) Prova fkv, RocksDB, fjall: RocksDB aprovat
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` La biblioteca compartida compilada es carrega amb dlopen (el mateix C
ABI, Rust build no enllaça a RocksDB), fjall utilitza `fjall 3` directament. Cadascun dels tres motors obre una biblioteca.
Avança a la mateixa biblioteca en l'ordre de les etapes: la càrrega massiva crea dades, i després len / lectura de punts / lectura d'interval /
Totes les supressions afecten les mateixes dades.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Omet la tercera columna si no hi ha RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Els números de la matriu de la pàgina d'inici només poden provenir d'una execució traçable:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI executa <id>"
```

`perf.yml` de GitHub Actions fa exactament això (instal·la `librocksdb-dev`, programa adaptadors, executa
Matriu de files de 25 milions, pengeu l'artefacte i ompliu directament la pàgina quan `backfill=true`).

### calibre

- **Ratio de dades/caché**: 25.000.000 de files × 116 B ≈ 2766 MiB de dades, 256 MiB de memòria cau per motor,
  ≈ 11:1, alineat amb la revisió oficial de RocksDB de 97 GiB de dades/6 GiB de memòria cau (≈ 16:1).
- **Maixa quantitat de memòria cau**: memòria cau de la pàgina de fkv (índex/dades mateix grup), memòria cau de blocs de RocksDB
  (`cache_index_and_filter_blocks=true`, mida del bloc 4 KiB), memòria cau de blocs pròpia de fjall
  Tots els pressupostos estan configurats amb el mateix nombre de bytes.
- **Calibre de persistència (la columna de comparació utilitza la configuració predeterminada de RocksDB, sense alineació)**: fkv 1 × pàgina de dades + per lot
  1×manifest `fdatasync` (isomòrfic a SyncData + AppendManifest de C++ EloqStore,
  Tampoc té interruptor d'apagada); la columna RocksDB utilitza la **configuració per defecte** (WAL activat, `WriteOptions.sync=false`,
  Sense fsync seqüencial), fjall també utilitza el valor predeterminat (`PersistMode::Buffer`). Per tant, la fase d'escriptura (escriptura única /
  La diferència entre escriptures per lots) és la **diferència de força de persistència**, no la diferència d'implementació, que s'ha d'indicar a la pàgina juntament amb el número.
  Si voleu analitzar "qui és més ràpid amb la mateixa força de sincronització", afegiu `--align-fsync` (obriu RocksDB amb sync=true,
  fjall utilitza SyncAll), aquesta opció no s'utilitza per a matrius publicades externament.
- **Dos controladors fkv**: `--fkv-driver store` (per defecte, connectat directament a un únic `Store` dins del procés i
  RocksDB / fjall també són trucades de biblioteca) i `--fkv-driver runtime` (mode de desplegament: un procés
  `--fkv-shards` fils de fragments, les sol·licituds s'envien per clau, alineades amb la lectura i escriptura pública de C++ EloqStore
  interfície). Cada sol·licitud d'aquest últim ha de passar per la cua + activació de fil creuat, nativa (macOS, el fitxer IO passa pel grup de fils)
  El temps de lectura és d'uns 20 µs/temps, que és un ordre de magnitud més lent que la connexió directa en procés; molt més barat a Linux/io_uring, però
  Encara un salt. La matriu de pàgines utilitza el calibre de la botiga i el calibre d'execució s'utilitza per a "formulari de desplegament vs C++
  Comparació de "Interfície pública" (la lectura d'un sol fil d'execució de fkv és d'uns 50 kops/s, C++ EloqStore FFI d'un sol fil
  fragment d'uns 21 cops/s).
- **Llegir calibre de concurrència**: número de fil lateral fkv = `min(readers, --read-max-concurrency)`, per defecte
  El límit superior és 4 (alineat amb la vCPU de la màquina d'avaluació), **no** "un fil per lector": el nivell superat només augmenta la cua;
  Memòria cau per fil = 256 MiB / nombre de fils (la quantitat total es fixa, no es tritura amb els lectors) i s'utilitza
  `FuturesUnordered` Manteniu les sol·licituds `--read-window` (16 per defecte) sobre la marxa. El costat de RocksDB és
  `readers` Cada `std::thread` llegeix simultàniament; bucle d'un sol fil al costat fjall (ignorant l'equip dels lectors,
  Només com a columna de referència). `--no-fkv-read-threads` pot tornar a degenerar a "N cadenes asíncrones intercalades en un sol temps d'execució".
  Aquesta és la profunditat del gasoducte, no la concurrència, i no és comparable entre els motors.
- **len()**: cap dels tres motors té interfícies de recompte O(1). Aquesta línia és la taxa de recompte de la taula completa, no la cerca O(1).