# Prestasi

`fkv_bench` Uji beban tulis, baca dan imbas pada ruang kunci tetap.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Petunjuk termasuk daya pemprosesan, kependaman pada setiap peringkat dan RSS puncak. keluarkan aliran kerja keluaran x86_64 /
`fkv_bench` binari untuk aarch64.

## Perlawanan tiga hala dengan RocksDB/fjall

`crate/perf/fkv_compare` Gunakan pemacu peringkat yang sama dan pengekodan kunci yang sama (subskrip big-endian 8-bait pada penghujung,
Susunan kamus adalah konsisten dengan susunan subskrip) Uji fkv, RocksDB, fjall: RocksDB lulus
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Pustaka kongsi yang disusun dimuatkan dengan dlopen (C
ABI, Rust build tidak memaut ke RocksDB), fjall menggunakan `fjall 3` secara langsung. Setiap satu daripada tiga enjin membuka perpustakaan.
Majukan pustaka yang sama dalam urutan peringkat: beban pukal mencipta data, dan kemudian len / bacaan titik / bacaan julat /
Semua pemadaman mempengaruhi data yang sama.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Langkau lajur ketiga jika tiada RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Nombor untuk matriks halaman utama hanya boleh datang daripada satu larian yang boleh dikesan:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

`perf.yml` Tindakan GitHub melakukan perkara ini dengan tepat (pasang `librocksdb-dev`, penyesuai program, jalankan
Matriks baris 25J, muat naik artifak dan isi semula halaman secara terus apabila `backfill=true`).

### berkaliber

- **Nisbah Data/Cache**: 25,000,000 baris × 116 B ≈ 2766 MiB data, 256 MiB cache setiap enjin,
  ≈ 11:1, sejajar dengan semakan rasmi RocksDB terhadap 97 data GiB/6 GiB cache (≈ 16:1).
- **Jumlah cache yang sama**: cache halaman fkv (kumpulan indeks/data yang sama), cache blok RocksDB
  (`cache_index_and_filter_blocks=true`, saiz blok 4 KiB), cache blok fjall sendiri
  Belanjawan semuanya ditetapkan kepada bilangan bait yang sama.
- **Kaliber kegigihan (lajur perbandingan menggunakan konfigurasi lalai RocksDB, tanpa penjajaran)**: fkv 1×halaman data + setiap kelompok
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Kedua-duanya tidak mempunyai suis mati); lajur RocksDB menggunakan **konfigurasi lalai** (WAL dihidupkan, `WriteOptions.sync=false`,
  Tiada fsync berurutan), fjall juga menggunakan lalai (`PersistMode::Buffer`). Oleh itu fasa menulis (tulisan tunggal /
  Perbezaan antara penulisan kelompok) ialah **perbezaan kekuatan kegigihan**, bukan perbezaan pelaksanaan, yang mesti dinyatakan pada halaman bersama-sama dengan nombor.
  Jika anda ingin menganalisis "siapa yang lebih pantas di bawah kekuatan penyegerakan yang sama", tambahkan `--align-fsync` (buka RocksDB dengan sync=true,
  fjall menggunakan SyncAll), pilihan ini tidak digunakan untuk matriks yang diterbitkan secara luaran.
- **Dua pemacu fkv**: `--fkv-driver store` (lalai, disambungkan terus kepada satu `Store` dalam proses dan
  RocksDB / fjall juga merupakan panggilan perpustakaan) dan `--fkv-driver runtime` (mod penempatan: proses
  `--fkv-shards` utas serpihan, permintaan dihalakan dengan kunci, sejajar dengan pembacaan dan penulisan awam C++ EloqStore
  antara muka). Setiap permintaan yang terakhir perlu melalui baris gilir + rentas benang bangun, asli (macOS, fail IO melalui kumpulan benang)
  Masa membaca adalah kira-kira 20 µs/masa, yang merupakan susunan magnitud lebih perlahan daripada sambungan langsung dalam proses; jauh lebih murah pada Linux/io_uring, tetapi
  Masih melompat. Matriks halaman menggunakan kaliber kedai, dan kaliber masa jalan digunakan untuk "bentuk penempatan vs C++
  Perbandingan "Antara Muka Awam" (masa jalanan fkv bacaan satu benang adalah kira-kira 50 kops/s, C++ EloqStore FFI satu benang
  serpihan kira-kira 21 kops/s).
- **Baca kaliber serentak**: nombor utas sisi fkv = `min(readers, --read-max-concurrency)`, lalai
  Had atas ialah 4 (diselaraskan dengan vCPU mesin penilaian), **bukan** "satu utas bagi setiap pembaca" - tahap yang melebihi hanya meningkatkan baris gilir;
  Cache setiap utas = 256 MiB / bilangan utas (jumlah keseluruhan adalah tetap, tidak dicincang dengan pembaca), dan digunakan
  `FuturesUnordered` Kekalkan permintaan `--read-window` (lalai 16) dengan segera. Bahagian RocksDB ialah
  `readers` Setiap `std::thread` dibaca serentak; gelung berulir tunggal pada sisi fjall (mengabaikan gear pembaca,
  Sebagai lajur rujukan sahaja). `--no-fkv-read-threads` boleh merosot kembali kepada "rantaian tak segerak N berjalin pada satu masa jalan".
  Itulah kedalaman saluran paip, bukan konkurensi, dan tidak setanding merentas enjin.
- **len()**: Tiada satu pun daripada tiga enjin yang mempunyai antara muka pengiraan O(1). Baris ini ialah kadar pengiraan jadual penuh, bukan carian O(1).