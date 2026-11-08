# Performa

`fkv_bench` Menguji beban tulis, baca, dan pindai pada ruang kunci tetap.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatornya mencakup throughput, latensi di setiap tahap, dan RSS puncak. rilis alur kerja rilis x86_64 /
`fkv_bench` biner untuk aarch64.

## Pertarungan tiga arah dengan RocksDB/fjall

`crate/perf/fkv_compare` Gunakan driver tahap yang sama dan pengkodean kunci yang sama (subskrip big-endian 8-byte di bagian akhir,
Urutan kamus konsisten dengan urutan subskrip) Uji fkv, RocksDB, fjall: RocksDB lulus
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Pustaka bersama yang dikompilasi dimuat dengan dlopen (C
ABI, Rust build tidak tertaut ke RocksDB), fjall menggunakan `fjall 3` secara langsung. Masing-masing dari tiga mesin membuka perpustakaan.
Maju di perpustakaan yang sama dalam urutan tahapan: pemuatan massal membuat data, lalu len / pembacaan titik / pembacaan rentang /
Semua penghapusan mempengaruhi data yang sama.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Lewati kolom ketiga jika tidak ada RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Angka-angka untuk matriks beranda hanya dapat berasal dari satu proses yang dapat dilacak:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI dijalankan <id>"
```

`perf.yml` GitHub Actions melakukan hal ini (instal `librocksdb-dev`, adaptor program, jalankan
Matriks baris 25 juta, unggah artefak, dan isi ulang halaman secara langsung ketika `backfill=true`).

### kaliber

- **Rasio Data/Cache**: 25.000.000 baris × 116 B ≈ 2766 MiB data, 256 MiB cache per mesin,
  ≈ 11:1, selaras dengan ulasan resmi RocksDB terhadap 97 GiB data/6 GiB cache (≈ 16:1).
- **Jumlah cache yang sama**: cache halaman fkv (indeks/kumpulan data yang sama), cache blok RocksDB
  (`cache_index_and_filter_blocks=true`, ukuran blok 4 KiB), cache blok fjall sendiri
  Semua anggaran ditetapkan ke jumlah byte yang sama.
- **Kaliber persistensi (kolom perbandingan menggunakan konfigurasi default RocksDB, tanpa penyelarasan)**: fkv 1×halaman data + per batch
  1×manifest `fdatasync` (Isomorfik ke SyncData + AppendManifest dari C++ EloqStore,
  Tidak ada tombol mati); kolom RocksDB menggunakan **konfigurasi default** (WAL aktif, `WriteOptions.sync=false`,
  Tidak ada fsync berurutan), fjall juga menggunakan default (`PersistMode::Buffer`). Oleh karena itu fase tulis (tulis tunggal /
  Perbedaan antara penulisan batch) adalah **perbedaan kekuatan persistensi**, bukan perbedaan implementasi, yang harus dicantumkan pada halaman beserta nomornya.
  Jika Anda ingin menganalisis "siapa yang lebih cepat dengan kekuatan sinkronisasi yang sama", tambahkan `--align-fsync` (buka RocksDB dengan sync=true,
  fjall menggunakan SyncAll), opsi ini tidak digunakan untuk matriks yang dipublikasikan secara eksternal.
- **Dua driver fkv**: `--fkv-driver store` (default, terhubung langsung ke satu `Store` dalam proses, dan
  RocksDB / fjall juga merupakan panggilan perpustakaan) dan `--fkv-driver runtime` (mode penerapan: suatu proses
  `--fkv-shards` thread shard, permintaan dirutekan berdasarkan kunci, selaras dengan pembacaan dan penulisan publik C++ EloqStore
  antarmuka). Setiap permintaan yang terakhir harus melalui antrian + bangun lintas-thread, asli (macOS, file IO melewati kumpulan thread)
  Waktu membaca sekitar 20 µs/waktu, yang merupakan urutan besarnya lebih lambat dibandingkan koneksi langsung dalam proses; jauh lebih murah di Linux/io_uring, tapi
  Masih lompatan. Matriks halaman menggunakan kaliber penyimpanan, dan kaliber runtime digunakan untuk "formulir penerapan vs C++
  Perbandingan "Antarmuka Publik" (pembacaan thread tunggal runtime fkv sekitar 50 kops/s, thread tunggal C++ EloqStore FFI
  pecahan sekitar 21 kops/dtk).
- **Baca kaliber konkurensi**: nomor thread samping fkv = `min(readers, --read-max-concurrency)`, default
  Batas atasnya adalah 4 (selaras dengan vCPU mesin evaluasi), **bukan** "satu thread per pembaca" - level yang terlampaui hanya menambah antrean;
  Cache per thread = 256 MiB / jumlah thread (jumlah total tetap, tidak diparut dengan pembaca), dan digunakan
  `FuturesUnordered` Pertahankan permintaan `--read-window` (default 16) dengan cepat. Sisi RocksDB adalah
  `readers` Setiap `std::thread` dibaca secara bersamaan; loop berulir tunggal di sisi fjall (mengabaikan perlengkapan pembaca,
  Sebagai kolom referensi saja). `--no-fkv-read-threads` dapat merosot kembali menjadi "rantai asinkron N yang disisipkan pada satu waktu proses".
  Itu adalah kedalaman pipeline, bukan konkurensi, dan tidak dapat dibandingkan antar mesin.
- **len()**: Tak satu pun dari ketiga mesin tersebut memiliki antarmuka penghitungan O(1). Baris ini adalah tingkat penghitungan tabel penuh, bukan pencarian O(1).