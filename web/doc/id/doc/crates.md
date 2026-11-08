# Crate

| lapisan | Crate | Tanggung jawab |
|---|---|---|
| foundation | `fkv_types` | ID, pola, konstanta |
| foundation | `fkv_error` | model kesalahan |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurasi dan penguraian INI |
| foundation | `fkv_metrics` | Penghitung dan Histogram |
| foundation | `fkv_failpoint` | injeksi kesalahan |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | kompresi mentah / kamus |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | menyelaraskan penyangga |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Nilai dan segmen besar |
| io | `fkv_io` | Ciri-ciri IO asinkron |
| io | `fkv_io_compio` | backend compio |
| io | `fkv_io_mem` | Bagian belakang memori |
| io | `fkv_io_sim` | Bagian belakang simulasi |
| io | `fkv_object` | Ciri-ciri penyimpanan objek |
| io | `fkv_object_s3` | bagian belakang S3 |
| io | `fkv_standby` | salinan siaga |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | loop kerja dan akses |
| engine | `fkv_store` | simpan siklus hidup dan perutean |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | memutar ulang dan membuka kembali |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | tolok ukur ujung ke ujung |