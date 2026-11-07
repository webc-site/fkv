# Crate

| slānis | Crate | Pienākumi |
|---|---|---|
| foundation | `fkv_types` | ID, modelis, konstante |
| foundation | `fkv_error` | kļūdas modelis |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurācija un INI parsēšana |
| foundation | `fkv_metrics` | Skaitītāji un histogrammas |
| foundation | `fkv_failpoint` | vainas injekcija |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | neapstrādāta / vārdnīcas saspiešana |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | izlīdzināt buferi |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Liela vērtība un segments |
| io | `fkv_io` | Asinhronās IO pazīmes |
| io | `fkv_io_compio` | kompio aizmugure |
| io | `fkv_io_mem` | Atmiņas aizmugure |
| io | `fkv_io_sim` | Simulācijas aizmugure |
| io | `fkv_object` | Objektu uzglabāšanas iezīmes |
| io | `fkv_object_s3` | S3 aizmugure |
| io | `fkv_standby` | gaidīšanas režīma kopija |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | darba cilpa un piekļuve |
| engine | `fkv_store` | veikala dzīves cikls un maršrutēšana |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | atkārtojiet un atveriet vēlreiz |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end etalons |