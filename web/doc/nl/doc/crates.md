# Crate

| laag | Crate | Verantwoordelijkheden |
|---|---|---|
| foundation | `fkv_types` | ID, patroon, constante |
| foundation | `fkv_error` | foutenmodel |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuratie en INI-parsering |
| foundation | `fkv_metrics` | Tellers en histogrammen |
| foundation | `fkv_failpoint` | fout injectie |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw/woordenboekcompressie |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | buffer uitlijnen |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Grote waarde en segment |
| io | `fkv_io` | Asynchrone IO-kenmerken |
| io | `fkv_io_compio` | compio-backend |
| io | `fkv_io_mem` | Geheugen-backend |
| io | `fkv_io_sim` | Simulatie-backend |
| io | `fkv_object` | Kenmerken van objectopslag |
| io | `fkv_object_s3` | S3-backend |
| io | `fkv_standby` | standby-kopie |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | werklus en toegang |
| engine | `fkv_store` | winkellevenscyclus en routing |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | opnieuw afspelen en opnieuw openen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end-benchmark |