# Performanse

`fkv_bench` Testirajte učitavanja pisanja, čitanja i skeniranja na fiksnom ključnom prostoru.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Pokazatelji uključuju propusnost, kašnjenje u svakoj fazi i vršni RSS. izdanje tijeka rada izdanje x86_64 /
`fkv_bench` binarni za aarch64.

## Trosmjerno podudaranje s RocksDB/fjall

`crate/perf/fkv_compare` Upotrijebite isti upravljački program faze i isto kodiranje ključa (8-bajtni veliki endian indeks na kraju,
Redoslijed u rječniku je u skladu s redoslijedom indeksa) Test fkv, RocksDB, fjall: RocksDB prošao
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Prevedena dijeljena biblioteka učitava se s dlopen (isti C
ABI, Rust build ne povezuje se s RocksDB), fjall izravno koristi `fjall 3`. Svaki od tri motora otvara biblioteku.
Napredovanje na istoj biblioteci prema redoslijedu faza: skupno učitavanje stvara podatke, a zatim len / očitanje točke / čitanje raspona /
Sva brisanja utječu na iste podatke.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Preskoči treći stupac ako nema RocksDB-a
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Brojevi za matricu početne stranice mogu doći samo iz jednog praćenja:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --stroj "..." --izvor "CI pokrenuti <id>"
```

`perf.yml` GitHub Actions radi upravo to (instalirajte `librocksdb-dev`, programirajte adaptere, pokrenite
25M matrice redaka, učitajte artefakt i izravno popunite stranicu kada `backfill=true`).

### kalibar

- **Omjer podataka/predmemorije**: 25.000.000 redaka × 116 B ≈ 2766 MiB podataka, 256 MiB predmemorije po motoru,
  ≈ 11:1, usklađeno sa RocksDB-ovom službenom recenzijom podataka od 97 GiB/6 GiB predmemorije (≈ 16:1).
- **Ista količina predmemorije**: fkv predmemorija stranica (indeks/podaci isti skup), RocksDB-ova predmemorija blokova
  (`cache_index_and_filter_blocks=true`, veličina bloka 4 KiB), fjall-ov vlastiti blok predmemorije
  Svi su proračuni postavljeni na isti broj bajtova.
- **Kalibar postojanosti (stupac za usporedbu koristi RocksDB zadanu konfiguraciju, bez poravnanja)**: fkv 1×stranica s podacima + po seriji
  1×manifest `fdatasync` (izomorfan SyncData + AppendManifest od C++ EloqStore,
  Ni jedno nema prekidač za isključivanje); stupac RocksDB koristi **zadanu konfiguraciju** (WAL uključen, `WriteOptions.sync=false`,
  Nema sekvencijalnog fsync-a), fjall također koristi zadani (`PersistMode::Buffer`). Stoga faza pisanja (pojedinačno pisanje /
  Razlika između serijskih zapisa) je **razlika u postojanosti**, a ne razlika u implementaciji, koja mora biti navedena na stranici zajedno s brojem.
  Ako želite analizirati "tko je brži pod istom snagom sinkronizacije", dodajte `--align-fsync` (otvorite RocksDB sa sync=true,
  fjall koristi SyncAll), ova opcija se ne koristi za eksterno objavljene matrice.
- **Dva fkv drajvera**: `--fkv-driver store` (zadano, izravno povezano s jednim `Store` unutar procesa, i
  RocksDB / fjall također su pozivi knjižnice) i `--fkv-driver runtime` (način implementacije: proces
  `--fkv-shards` shard threads, zahtjevi se usmjeravaju prema ključu, usklađeni s javnim čitanjem i pisanjem C++ EloqStorea
  sučelje). Svaki zahtjev potonjeg treba proći kroz red čekanja + buđenje unakrsne niti, nativno (macOS, datoteka IO prolazi kroz skup niti)
  Vrijeme očitavanja je oko 20 µs/vrijeme, što je red veličine sporije od izravne veze u procesu; mnogo jeftiniji na Linux/io_uring, ali
  Ipak skok. Matrica stranice koristi kalibar trgovine, a kalibar vremena izvođenja koristi se za "formu za implementaciju u odnosu na C++
  Usporedba "javnog sučelja" (fkv runtime jednonitno čitanje je oko 50 kops/s, C++ EloqStore FFI jednonitno
  krhotina oko 21 kops/s).
- **Kalibar paralelnosti čitanja**: fkv bočni broj niti = `min(readers, --read-max-concurrency)`, zadano
  Gornja granica je 4 (usklađena s vCPU-om stroja za procjenu), **ne** "jedna nit po čitaču" - prekoračena razina samo povećava red čekanja;
  Predmemorija po niti = 256 MiB / broj niti (ukupna količina je fiksna, nije usitnjena s čitačima) i koristi se
  `FuturesUnordered` Održavanje `--read-window` (zadano 16) zahtjeva u hodu. RocksDB strana je
  `readers` Svaki `std::thread` čita istovremeno; jednonitna petlja na fjall strani (zanemarujući opremu za čitanje,
  Samo kao referentni stupac). `--no-fkv-read-threads` može degenerirati natrag u "isprepletenih N asinkronih lanaca u jednom vremenu izvođenja".
  To je dubina cjevovoda, a ne istovremenost, i nije usporediva među motorima.
- **len()**: Nijedan od tri motora nema O(1) sučelja za brojanje. Ovaj redak je puna stopa brojanja tablice, a ne O(1) pretraživanje.