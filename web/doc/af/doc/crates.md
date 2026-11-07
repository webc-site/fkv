# Crate

| laag | Crate | Verantwoordelikhede |
|---|---|---|
| foundation | `fkv_types` | ID, patroon, konstante |
| foundation | `fkv_error` | fout model |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurasie en INI-ontleding |
| foundation | `fkv_metrics` | Tellers en histogramme |
| foundation | `fkv_failpoint` | foutinspuiting |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | rou / woordeboek kompressie |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | belyn buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Groot waarde en segment |
| io | `fkv_io` | Asinchrone IO-eienskappe |
| io | `fkv_io_compio` | samestelling agterkant |
| io | `fkv_io_mem` | Geheue agterkant |
| io | `fkv_io_sim` | Simulasie agterkant |
| io | `fkv_object` | Eienskappe van objekberging |
| io | `fkv_object_s3` | S3 agterkant |
| io | `fkv_standby` | bystandkopie |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | werklus en toegang |
| engine | `fkv_store` | stoor lewensiklus en roetes |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | herspeel en heropen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-tot-end maatstaf |