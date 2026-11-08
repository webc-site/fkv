# Performanse

`fkv_bench` Testirajte učitavanje pisanja, čitanja i skeniranja na fiksnom ključnom prostoru.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatori uključuju propusnost, kašnjenje u svakoj fazi i vršni RSS. izdanje toka posla izdanje x86_64 /
`fkv_bench` binarni za aarch64.

## Trosmjerna utakmica sa RocksDB/fjall

`crate/perf/fkv_compare` Koristite isti drajver faze i isto kodiranje ključa (8-bajtni big-endian subscript na kraju,
Redoslijed rječnika je u skladu s redoslijedom indeksa) Test fkv, RocksDB, fjall: RocksDB prošao
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Generisana zajednička biblioteka se učitava pomoću dlopen (isti C
ABI, Rust build se ne povezuje sa RocksDB), fjall koristi `fjall 3` direktno. Svaki od tri motora otvara biblioteku.
Napredujte na istoj biblioteci po redoslijedu faza: masovno opterećenje kreira podatke, a zatim len / očitavanje točke / čitanje raspona /
Sva brisanja utiču na iste podatke.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Preskoči treću kolonu ako nema RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

brojevi na glavnoj matrici mogu doći samo iz jednog pratljivog pokretanja:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions' `perf.yml` radi upravo to (instalirajte `librocksdb-dev`, programske adaptere, pokrenite
25M redova matrice, učitaj artifact, i direktno popuni stranicu u `backfill=true`). 

### kalibar

- **Odnos podataka/keš memorije**: 25.000.000 redova × 116 B ≈ 2766 MiB podataka, 256 MiB keš memorije po mašini,
  ≈ 11:1, u skladu sa službenim pregledom RocksDB-a 97 GiB podataka/6 GiB keš memorije (≈ 16:1).
- **Ista količina keš memorije**: fkv-ova predmemorija stranica (indeks/podaci isti bazen), RocksDB-ova blok keš memorija
  (`cache_index_and_filter_blocks=true`, veličina bloka 4 KiB), fjall-ov vlastiti blok keš
  Budžeti su svi postavljeni na isti broj bajtova.
- **Kalibar postojanosti (kolona za poređenje koristi zadanu konfiguraciju RocksDB, bez poravnanja)**: fkv 1×stranica sa podacima + po seriji
  1×manifest `fdatasync` (izomorfno SyncData + AppendManifest C++ EloqStore,
  Nijedna nema prekidač za isključivanje); RocksDB kolona koristi **podrazumevanu konfiguraciju** (WAL uključen, `WriteOptions.sync=false`,
  Nema sekvencijalne fsync), fjall također koristi zadanu (`PersistMode::Buffer`). Stoga faza pisanja (jednokratno upisivanje /
  Razlika između grupnog upisivanja) je **razlika u snazi ​​postojanosti**, a ne razlika u implementaciji, koja mora biti navedena na stranici zajedno sa brojem.
  Ako želite analizirati "ko je brži pod istom snagom sinhronizacije", dodajte `--align-fsync` (otvorite RocksDB sa sync=true,
  fjall koristi SyncAll), ova opcija se ne koristi za eksterno objavljene matrice.
- **Dva fkv drajvera**: `--fkv-driver store` (podrazumevano, direktno povezan sa jednim `Store` unutar procesa, i
  RocksDB / fjall su također pozivi biblioteke) i `--fkv-driver runtime` (način implementacije: proces
  `--fkv-shards` shard threads, zahtjevi se rutiraju po ključu, usklađeni s javnim čitanjem i pisanjem C++ EloqStore
  interfejs). Svaki zahtjev potonjeg mora proći kroz red čekanja + buđenje unakrsnih niti, izvorni (macOS, datoteka IO prolazi kroz skup niti)
  Vrijeme čitanja je oko 20 µs/vreme, što je red veličine sporije od direktne veze u procesu; mnogo jeftinije na Linux/io_uring, ali
  Još uvek skok. Matrica stranice koristi kalibar trgovine, a kalibar vremena izvođenja se koristi za "formac za implementaciju vs C++
  Poređenje "Public Interface" (fkv runtime single-thread čitanje je oko 50 kops/s, C++ EloqStore FFI single-thread
  krhotina oko 21 kops/s).
- **Kalibar istodobnosti čitanja**: fkv broj bočne niti = `min(readers, --read-max-concurrency)`, zadano
  Gornja granica je 4 (usklađeno sa vCPU mašine za evaluaciju), **ne** "jedna nit po čitaču" - prekoračenje nivoa samo povećava red čekanja;
  Keš memorija po niti = 256 MiB / broj niti (ukupna količina je fiksna, nije usitnjena čitačima) i korištena
  `FuturesUnordered` Zadržite `--read-window` (podrazumevano 16) zahtjeve u pokretu. RocksDB strana je
  `readers` Svaki `std::thread` čita istovremeno; jednonitna petlja na fjall strani (zanemarujući opremu za čitanje,
  Samo kao referentna kolona). `--no-fkv-read-threads` može se vratiti nazad u "isprepletenih N asinhronih lanaca u jednom runtimeu".
  To je dubina cjevovoda, a ne istovremenost, i nije uporediva među motorima.
- **len()**: Nijedan od tri mehanizma nema O(1) interfejs za brojanje. Ova linija je stopa brojanja pune tabele, a ne O(1) traženje.