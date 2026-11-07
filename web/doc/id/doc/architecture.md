# Arsitektur

## waktu proses

Setiap pecahan memiliki thread OS eksklusif dan runtime compio. Permintaan masuk melalui antrian masuk,
Tiket masuk dengan mesin shard state, diselesaikan melalui tanda terima oneshot.

## penyimpanan

Mesinnya menggunakan B+tree copy-on-write. halaman data, halaman indeks, rantai luapan dan
Manifes diserialisasikan ke dalam file segmen. cabang diidentifikasi oleh `(branch, term)` dan dipegang oleh masing-masing cabang
manifes dan pemetaan snapshot.

## IO

`fkv_io` Mendefinisikan sifat IO asinkron. `fkv_io_compio` Diimplementasikan dengan compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` menyediakan memori dan backend injeksi kesalahan, `fkv_object_s3` menyediakan
ujung belakang objek S3.

## kegigihan

Manifes mencatat halaman root, root TTL, metadata cabang, dan pemetaan. Saat Dibuka Kembali
Pertama-tama putar ulang manifes, lalu buat ulang pembuat peta halaman, lalu mulai melayani permintaan.