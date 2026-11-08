# Crate

| vrstva | Crate | Zodpovednosti |
|---|---|---|
| foundation | `fkv_types` | ID, vzor, ​​konštanta |
| foundation | `fkv_error` | chybový model |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurácia a analýza INI |
| foundation | `fkv_metrics` | Počítadlá a histogramy |
| foundation | `fkv_failpoint` | poruchové vstrekovanie |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / slovníková kompresia |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | zarovnať vyrovnávaciu pamäť |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Veľká hodnota a segment |
| io | `fkv_io` | Asynchrónne IO vlastnosti |
| io | `fkv_io_compio` | Compio backend |
| io | `fkv_io_mem` | Pamäťový backend |
| io | `fkv_io_sim` | Simulačný backend |
| io | `fkv_object` | Vlastnosti ukladania predmetov |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | pohotovostná kópia |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | pracovná slučka a prístup |
| engine | `fkv_store` | životný cyklus úložiska a smerovanie |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | prehrať a znova otvoriť |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end benchmark |