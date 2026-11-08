# Crate

| lapisan | Crate | Tanggung jawab |
|---|---|---|
| foundation | `fkv_types` | ID, pola, konstan |
| foundation | `fkv_error` | model kesalahan |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurasi lan parsing INI |
| foundation | `fkv_metrics` | Counter lan Histogram |
| foundation | `fkv_failpoint` | injeksi fault |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | kompresi mentah / kamus |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | nyelarasake buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Nilai gedhe lan segmen |
| io | `fkv_io` | Sifat IO asinkron |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Backend memori |
| io | `fkv_io_sim` | Simulasi backend |
| io | `fkv_object` | Sifat panyimpenan obyek |
| io | `fkv_object_s3` | S3 mburi |
| io | `fkv_standby` | salinan siaga |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | daur ulang lan akses |
| engine | `fkv_store` | nyimpen siklus urip lan nuntun |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | muter maneh lan mbukak maneh |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | pathokan end-to-end |