# Crate

| lag | Crate | Ansvar |
|---|---|---|
| foundation | `fkv_types` | ID, mønster, konstant |
| foundation | `fkv_error` | fejlmodel |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguration og INI-parsing |
| foundation | `fkv_metrics` | Tællere og histogrammer |
| foundation | `fkv_failpoint` | fejlindsprøjtning |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | rå / ordbogskomprimering |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | juster buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Stor værdi og segment |
| io | `fkv_io` | Asynkrone IO-træk |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Hukommelse backend |
| io | `fkv_io_sim` | Simulering backend |
| io | `fkv_object` | Objektlagringsegenskaber |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | standby kopi |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | arbejdsløkke og adgang |
| engine | `fkv_store` | butiks livscyklus og routing |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | afspille og genåbne |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end benchmark |