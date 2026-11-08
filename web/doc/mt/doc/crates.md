# Crate

| saff | Crate | Responsabbiltajiet |
|---|---|---|
| foundation | `fkv_types` | ID, mudell, kostanti |
| foundation | `fkv_error` | mudell ta' żball |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurazzjoni u parsing INI |
| foundation | `fkv_metrics` | Counters u Istogrammi |
| foundation | `fkv_failpoint` | injezzjoni tal-ħsara |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | kompressjoni mhux maħduma / dizzjunarju |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | tallinja buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Valur kbir u segment |
| io | `fkv_io` | Karatteristiċi IO asinkroniċi |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Backend tal-memorja |
| io | `fkv_io_sim` | Backend tas-simulazzjoni |
| io | `fkv_object` | Karatteristiċi tal-ħażna tal-oġġett |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | kopja standby |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | linja tax-xogħol u aċċess |
| engine | `fkv_store` | ċiklu tal-ħajja tal-maħżen u rotta |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | replay u terġa' tiftaħ |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | benchmark minn tarf sa tarf |