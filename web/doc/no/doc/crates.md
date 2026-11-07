# Crate

| lag | Crate | Ansvar |
|---|---|---|
| foundation | `fkv_types` | ID, mønster, konstant |
| foundation | `fkv_error` | feilmodell |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurasjon og INI-parsing |
| foundation | `fkv_metrics` | Tellere og histogrammer |
| foundation | `fkv_failpoint` | feilinjeksjon |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | rå / ordbok komprimering |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | juster buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Stor verdi og segment |
| io | `fkv_io` | Asynkrone IO-trekk |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Minne bakenden |
| io | `fkv_io_sim` | Simuleringsbackend |
| io | `fkv_object` | Objektlagringsegenskaper |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | standby-kopi |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | arbeidsløkke og tilgang |
| engine | `fkv_store` | lagre livssyklus og ruting |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | replay og gjenåpne |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | ende-til-ende benchmark |