# Crate

| safu | Crate | Majukumu |
|---|---|---|
| foundation | `fkv_types` | ID, muundo, mara kwa mara |
| foundation | `fkv_error` | mfano wa makosa |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Usanidi na uchanganuzi wa INI |
| foundation | `fkv_metrics` | Counters na Histograms |
| foundation | `fkv_failpoint` | sindano ya kosa |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | mbichi / mgandamizo wa kamusi |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | panga bafa |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Thamani kubwa na sehemu |
| io | `fkv_io` | Sifa za Asynchronous za IO |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Nyuma ya kumbukumbu |
| io | `fkv_io_sim` | Simulation backend |
| io | `fkv_object` | Tabia za uhifadhi wa kitu |
| io | `fkv_object_s3` | Sehemu ya nyuma ya S3 |
| io | `fkv_standby` | nakala ya kusubiri |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | kitanzi cha kazi na ufikiaji |
| engine | `fkv_store` | kuhifadhi mzunguko wa maisha na uelekezaji |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | cheza tena na ufungue tena |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | alama ya mwisho hadi mwisho |