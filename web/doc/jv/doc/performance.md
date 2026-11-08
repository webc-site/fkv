# Kinerja

`fkv_bench` Tes tulis, waca, lan pindai muatan ing papan tombol tetep.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikator kalebu throughput, latensi ing saben tahap, lan RSS puncak. rilis alur kerja rilis x86_64 /
`fkv_bench` binar kanggo aarch64.

## Pertandhingan telung arah karo RocksDB / fjall

`crate/perf/fkv_compare` Gunakake driver panggung sing padha lan enkoding tombol sing padha (subskrip big-endian 8-byte ing pungkasan,
Urutan kamus jumbuh karo urutan subskrip) Tes fkv, RocksDB, fjall: RocksDB lulus
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Pustaka bareng sing dikompilasi dimuat karo dlopen (C
ABI, Rust build ora nyambung menyang RocksDB), fjall nggunakake `fjall 3` langsung. Saben telung mesin mbukak perpustakaan.
Maju ing perpustakaan sing padha ing urutan tahapan: beban akeh nggawe data, banjur len / point reading / range reading /
Pambusakan kabeh mengaruhi data sing padha.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Skip kolom katelu yen ora ana RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Angka kanggo matriks homepage mung bisa teka saka siji run sing bisa dilacak:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions' `perf.yml` nindakake persis iki (instal `librocksdb-dev`, adaptor program, run
Matriks baris 25M, upload artefak, lan langsung isi ulang kaca nalika `backfill=true`).

### kaliber

- **Data/Rasio Cache**: 25.000.000 baris × 116 B ≈ 2766 MiB data, 256 MiB cache saben mesin,
  ≈ 11: 1, selaras karo review resmi RocksDB babagan 97 data GiB / 6 cache GiB (≈ 16: 1).
- **Jumlah cache sing padha**: cache kaca fkv (indeks/data pool sing padha), cache blok RocksDB
  (`cache_index_and_filter_blocks=true`, ukuran blok 4 KiB), cache blok fjall dhewe
  Anggaran kabeh disetel menyang jumlah bita sing padha.
- **Kaliber ketekunan (kolom perbandingan nggunakake konfigurasi standar RocksDB, tanpa keselarasan)**: fkv 1×kaca data + saben kumpulan
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Ora ana saklar mati); kolom RocksDB nggunakake **konfigurasi standar** (WAL on, `WriteOptions.sync=false`,
  Ora ana fsync urutan), fjall uga nggunakake standar (`PersistMode::Buffer`). Mulane fase nulis (tulis tunggal /
  Bentenipun antarane nulis kumpulan) iku ** prabédan kekuatan kegigihan **, ora prabédan implementasine, kang kudu nyatakake ing kaca bebarengan karo nomer.
  Yen sampeyan pengin nganalisa "sapa sing luwih cepet miturut kekuwatan sinkronisasi sing padha", tambahake `--align-fsync` (mbukak RocksDB kanthi sync=true,
  fjall migunakake SyncAll), opsi iki ora digunakake kanggo matriks sing diterbitake sacara eksternal.
- **Loro driver fkv**: `--fkv-driver store` (standar, disambungake langsung menyang siji `Store` ing proses kasebut, lan
  RocksDB / fjall uga minangka panggilan perpustakaan) lan `--fkv-driver runtime` (mode penyebaran: proses
  `--fkv-shards` utas beling, panjalukan dirutekan nganggo kunci, selaras karo maca lan nulis umum C++ EloqStore
  antarmuka). Saben panyuwunan sing terakhir kudu ngliwati antrian + cross-thread wake-up, native (macOS, file IO ngliwati kolam benang)
  Wektu maca kira-kira 20 µs / wektu, yaiku urutan gedhene luwih alon tinimbang sambungan langsung ing proses; luwih murah ing Linux / io_uring, nanging
  Isih mlumpat. Matriks kaca nggunakake kaliber nyimpen, lan kaliber runtime digunakake kanggo "wangun penyebaran vs C ++
  Perbandingan "Antarmuka Umum" (fkv runtime single-thread reading kira-kira 50 kops/s, C++ EloqStore FFI single-thread
  pecahan babagan 21 kops / s).
- **Waca kaliber konkurensi**: nomer utas sisih fkv = `min(readers, --read-max-concurrency)`, standar
  Watesan ndhuwur iku 4 (selaras karo vCPU mesin evaluasi), ** ora ** "siji thread saben maca" - tingkat ngluwihi mung nambah antrian;
  Cache saben utas = 256 MiB / jumlah utas (jumlah total tetep, ora dipotong karo pembaca), lan digunakake
  `FuturesUnordered` Tansah panjalukan `--read-window` (standar 16) kanthi cepet. sisih RocksDB punika
  `readers` Saben `std::thread` diwaca bebarengan; daur ulang siji-threaded ing sisih fjall (ora nggatekake peralatan maca,
  Minangka kolom referensi mung). `--no-fkv-read-threads` bisa degenerate bali menyang "interleaved N rantai asinkron ing siji runtime".
  Iku ambane pipeline, ora concurrency, lan ora iso dibandhingke antarane mesin.
- ** len () **: Ora ana telung mesin duwe antarmuka pancacahan O (1). Baris iki minangka tingkat pancacahan tabel lengkap, dudu O (1) goleki.