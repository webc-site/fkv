# Seni bina

## masa jalan

Setiap serpihan mempunyai urutan OS eksklusif dan masa jalan kompilasi. Permintaan masuk melalui baris gilir masuk,
Kemasukan melalui mesin keadaan shard, diselesaikan melalui resit oneshot.

## penyimpanan

Enjinnya menggunakan pokok B+ copy-on-write. halaman data, halaman indeks, rantaian limpahan dan
Manifes disiri ke dalam fail segmen. cawangan dikenal pasti oleh `(branch, term)` dan dipegang oleh setiap satu
gambar nyata dan pemetaan.

## IO

`fkv_io` Mentakrifkan sifat IO tak segerak. `fkv_io_compio` Dilaksanakan dengan compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` menyediakan memori dan bahagian belakang suntikan kesalahan, `fkv_object_s3` menyediakan
Bahagian belakang objek S3.

## kegigihan

Manifes merekodkan halaman akar, akar TTL, metadata cawangan dan pemetaan. Apabila Dibuka Semula
Main semula manifes dahulu, kemudian bina semula pemeta halaman, dan kemudian mula melayani permintaan.