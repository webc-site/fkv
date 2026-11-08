# Performanță

`fkv_bench` Testați încărcările de scriere, citire și scanare pe un spațiu fix pentru chei.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indicatorii includ debitul, latența în fiecare etapă și RSS de vârf. lansează lansarea fluxului de lucru x86_64 /
`fkv_bench` binar pentru aarch64.

## Confruntare în trei cu RocksDB/fjall

`crate/perf/fkv_compare` Folosiți același driver de scenă și aceeași codare cheie (indice big-endian de 8 octeți la sfârșit,
Ordinea dicționarului este în concordanță cu ordinea indicelui) Testul fkv, RocksDB, fjall: RocksDB a trecut
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Biblioteca partajată compilată este încărcată cu dlopen (același C
ABI, Rust build nu se conectează la RocksDB), fjall folosește `fjall 3` direct. Fiecare dintre cele trei motoare deschide o bibliotecă.
Avansați pe aceeași bibliotecă în ordinea etapelor: încărcarea în vrac creează date, apoi len / citire punct / citire interval /
Toate ștergerile afectează aceleași date.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Omite a treia coloană dacă nu există RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Numerele pentru matricea paginii de pornire pot proveni doar dintr-o singură rulare urmăribilă:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

`perf.yml` din GitHub Actions face exact acest lucru (instalați `librocksdb-dev`, adaptoare de program, rulați
matrice de 25 de milioane de rânduri, încărcați artefact și completați direct pagina când `backfill=true`).

### calibru

- **Raport date/cache**: 25.000.000 de rânduri × 116 B ≈ 2766 MiB de date, 256 MiB de cache per motor,
  ≈ 11:1, aliniat cu revizuirea oficială RocksDB a 97 GiB de date/6 GiB cache (≈ 16:1).
- **Aceeași cantitate de cache**: cache-ul paginii fkv (index/date același pool), cache-ul bloc al RocksDB
  (`cache_index_and_filter_blocks=true`, dimensiunea blocului 4 KiB), memoria cache a blocurilor proprie fjall
  Bugetele sunt toate setate la același număr de octeți.
- **Calibru de persistență (coloana de comparație folosește configurația implicită RocksDB, fără aliniere)**: fkv 1×pagină de date + per lot
  1×manifest `fdatasync` (izomorf la SyncData + AppendManifest din C++ EloqStore,
  Niciunul nu are întrerupător de oprire); coloana RocksDB folosește **configurația implicită** (WAL activat, `WriteOptions.sync=false`,
  Fără fsync secvențial), fjall folosește și varianta implicită (`PersistMode::Buffer`). Prin urmare, faza de scriere (scriere unică /
  Diferența dintre scrierile în lot) este **diferența de rezistență la persistență**, nu diferența de implementare, care trebuie menționată pe pagină împreună cu numărul.
  Dacă doriți să analizați „cine este mai rapid cu aceeași putere de sincronizare”, adăugați `--align-fsync` (deschideți RocksDB cu sync=true,
  fjall folosește SyncAll), această opțiune nu este utilizată pentru matricele publicate extern.
- **Două drivere fkv**: `--fkv-driver store` (implicit, conectat direct la un singur `Store` în cadrul procesului și
  RocksDB / fjall sunt, de asemenea, apeluri de bibliotecă) și `--fkv-driver runtime` (mod de implementare: un proces
  `--fkv-shards` fire de execuție, solicitările sunt direcționate după cheie, aliniate cu citirea și scrierea publică a C++ EloqStore
  interfață). Fiecare cerere a acestuia din urmă trebuie să treacă prin coadă + trezire încrucișată, nativă (macOS, fișierul IO trece prin pool-ul de fire)
  Timpul de citire este de aproximativ 20 µs/timp, ceea ce este cu un ordin de mărime mai lent decât conexiunea directă în proces; mult mai ieftin pe Linux/io_uring, dar
  Încă un salt. Matricea paginii folosește calibrul magazinului, iar calibrul de rulare este folosit pentru „formularul de implementare vs C++
  Comparație între „Interfața publică” (fkv runtime single-thread citirea este de aproximativ 50 kops/s, C++ EloqStore FFI single-thread
  ciob aproximativ 21 cops/s).
- **Citiți calibru concurent**: numărul firului lateral fkv = `min(readers, --read-max-concurrency)`, implicit
  Limita superioară este 4 (aliniată cu vCPU-ul mașinii de evaluare), **nu** „un fir per cititor” - nivelul depășit nu face decât să mărească coada;
  Cache per thread = 256 MiB / număr de fire (cantitatea totală este fixă, nu mărunțită cu cititoare) și utilizată
  `FuturesUnordered` Păstrați cererile `--read-window` (implicit 16) din mers. Partea RocksDB este
  `readers` Fiecare `std::thread` citește simultan; buclă cu un singur fir pe partea fjall (ignorând echipamentul cititorului,
  Numai ca coloană de referință). `--no-fkv-read-threads` poate degenera înapoi la „N lanțuri asincrone intercalate pe un singur timp de rulare”.
  Aceasta este adâncimea conductei, nu concurența și nu este comparabilă între motoare.
- **len()**: Niciunul dintre cele trei motoare nu are interfețe de numărare O(1). Această linie este rata de numărare a tabelului complet, nu căutarea O(1).