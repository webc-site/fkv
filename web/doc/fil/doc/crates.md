# Crate

| layer | Crate | Mga responsibilidad |
|---|---|---|
| foundation | `fkv_types` | ID, pattern, pare-pareho |
| foundation | `fkv_error` | modelo ng error |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuration at INI parsing |
| foundation | `fkv_metrics` | Mga Counter at Histogram |
| foundation | `fkv_failpoint` | fault injection |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / compression ng diksyunaryo |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | ihanay ang buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Malaking halaga at segment |
| io | `fkv_io` | Asynchronous na mga katangian ng IO |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Backend ng memory |
| io | `fkv_io_sim` | Simulation backend |
| io | `fkv_object` | Mga katangian ng imbakan ng bagay |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | naka-standby na kopya |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | work loop at access |
| engine | `fkv_store` | ikot ng buhay ng tindahan at pagruruta |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | i-replay at muling buksan |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end na benchmark |