# Performanca

`fkv_bench` Testoni ngarkesat e shkrimit, leximit dhe skanimit në një hapësirë ​​fikse të çelësit.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Treguesit përfshijnë xhiron, vonesën në çdo fazë dhe kulmin e RSS. lëshimi i rrjedhës së punës lëshimi x86_64 /
`fkv_bench` binar për aarch64.

## Përputhje me tre drejtime me RocksDB/fjall

`crate/perf/fkv_compare` Përdorni të njëjtin drejtues të skenës dhe të njëjtin kodim të çelësit (abonim i madh endian 8 byte në fund,
Renditja e fjalorit është në përputhje me rendin e nënshkrimit) Test fkv, RocksDB, fjall: RocksDB kaloi
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Biblioteka e përbashkët e përpiluar ngarkohet me dlopen (e njëjta C
ABI, ndërtimi Rust nuk lidhet me RocksDB), fjall përdor `fjall 3` drejtpërdrejt. Secili nga tre motorët hap një bibliotekë.
Përparoni në të njëjtën bibliotekë në rendin e fazave: ngarkesa me shumicë krijon të dhëna, dhe më pas lexim / leximi i pikës / leximi i diapazonit /
Fshirjet ndikojnë të gjitha të njëjtat të dhëna.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Kalo kolonën e tretë nëse nuk ka RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Numrat për matricën e faqes kryesore mund të vijnë vetëm nga një ekzekutim i gjurmueshëm:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --burimi "CI run <id>"
```

GitHub Actions' `perf.yml` bën pikërisht këtë (instaloni `librocksdb-dev`, përshtatësit e programit, ekzekutoni
Matrica e rreshtit 25 milion, ngarkoni objekte dhe plotësoni drejtpërdrejt faqen kur `backfill=true`).

### kalibrit

- **Raporti i të dhënave/cache**: 25,000,000 rreshta × 116 B ≈ 2766 MiB të dhëna, 256 MiB cache për motor,
  ≈ 11:1, në përputhje me rishikimin zyrtar të RocksDB për 97 GiB të dhëna/6 GiB cache (≈ 16:1).
- **E njëjta sasi cache**: cache e faqeve të fkv (i njëjti grup i indeksit/të dhënave), memoria e bllokut e RocksDB
  (`cache_index_and_filter_blocks=true`, madhësia e bllokut 4 KiB), memoria e bllokut e vetë fjall
  Të gjitha buxhetet janë vendosur në të njëjtin numër bajtësh.
- **Kalibri i qëndrueshmërisë (kolona e krahasimit përdor konfigurimin e paracaktuar të RocksDB, pa shtrirje)**: fkv 1×faqe të dhënash + për grup
  1×manifest `fdatasync` (Izomorfik në SyncData + AppendManifest i C++ EloqStore,
  Asnjëra nuk ka një ndërprerës fikje); kolona RocksDB përdor **konfigurimin e parazgjedhur** (WAL aktiv, `WriteOptions.sync=false`,
  Nuk ka fsync vijues), fjall përdor gjithashtu parazgjedhjen (`PersistMode::Buffer`). Prandaj faza e shkrimit (shkrim i vetëm /
  Dallimi midis shkrimeve në grup) është **diferenca e forcës së qëndrueshmërisë**, jo diferenca e zbatimit, e cila duhet të deklarohet në faqe së bashku me numrin.
  Nëse dëshironi të analizoni "kush është më i shpejtë me të njëjtën forcë sinkronizimi", shtoni `--align-fsync` (hap RocksDB me sync=true,
  fjall përdor SyncAll), ky opsion nuk përdoret për matricat e publikuara nga jashtë.
- **Dy drejtues fkv**: `--fkv-driver store` (i parazgjedhur, i lidhur drejtpërdrejt me një `Store` të vetëm brenda procesit, dhe
  RocksDB / fjall janë gjithashtu thirrje të bibliotekës) dhe `--fkv-driver runtime` (modaliteti i vendosjes: një proces
  `--fkv-shards` threads të copëzuar, kërkesat drejtohen me çelës, në linjë me leximin dhe shkrimin publik të C++ EloqStore
  ndërfaqe). Çdo kërkesë e kësaj të fundit duhet të kalojë në radhë + zgjimi ndërthread, vendas (macOS, skedari IO kalon nëpër grupin e temave)
  Koha e leximit është rreth 20 µs/kohë, që është një renditje e madhësisë më e ngadaltë se lidhja direkte në proces; shumë më lirë në Linux/io_uring, por
  Ende një kërcim. Matrica e faqes përdor kalibrin e dyqanit, dhe kalibri i kohës së funksionimit përdoret për "formën e vendosjes kundrejt C++
  Krahasimi i "Ndërfaqes publike" (leximi me një fije të vetme në kohëzgjatje fkv është rreth 50 kops/s, C++ EloqStore FFI me një fije të vetme
  copëza rreth 21 kops/s).
- **Lexo kalibrin e konkurencës**: numri i fillit anësor fkv = `min(readers, --read-max-concurrency)`, i parazgjedhur
  Kufiri i sipërm është 4 (në linjë me makinën e vlerësimit vCPU), **jo** "një fije për lexues" - niveli i tejkaluar vetëm rrit radhën;
  Cache për thread = 256 MiB / numri i threads (shuma totale është fikse, nuk copëtohet me lexues) dhe përdoret
  `FuturesUnordered` Mbani kërkesat `--read-window` (parazgjedhja 16) në fluturim. Ana RocksDB është
  `readers` Çdo `std::thread` lexon njëkohësisht; lak me një fije në anën fjall (duke injoruar ingranazhet e lexuesve,
  Vetëm si kolonë referimi). `--no-fkv-read-threads` mund të degjenerojë përsëri në "N zinxhirë asinkronë të ndërthurur në një kohë të vetme ekzekutimi".
  Kjo është thellësia e tubacionit, jo konkurencë dhe nuk është e krahasueshme me motorët.
- **len()**: Asnjë nga tre motorët nuk ka ndërfaqe numërimi O(1). Kjo linjë është norma e plotë e numërimit të tabelës, jo kërkimi O(1).