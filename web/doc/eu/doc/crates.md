# Crate

| geruza | Crate | Erantzukizunak |
|---|---|---|
| foundation | `fkv_types` | ID, eredua, konstantea |
| foundation | `fkv_error` | errore-eredua |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurazioa eta INI analizatzea |
| foundation | `fkv_metrics` | Kontagailuak eta histogramak |
| foundation | `fkv_failpoint` | matxura injekzioa |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | gordina / hiztegi konpresioa |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | lerrokatu buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Balio eta segmentu handia |
| io | `fkv_io` | IO ezaugarri asinkronoak |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Memoria backend |
| io | `fkv_io_sim` | Simulazio backend-a |
| io | `fkv_object` | Objektuak biltegiratzeko ezaugarriak |
| io | `fkv_object_s3` | S3 backend-a |
| io | `fkv_standby` | egonean kopia |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | lan-begizta eta sarbidea |
| engine | `fkv_store` | dendaren bizi-zikloa eta bideratzea |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | errepikatu eta berriro ireki |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | amaierako erreferentea |