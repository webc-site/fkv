# Crate

| слој | Crate | Одговорности |
|---|---|---|
| foundation | `fkv_types` | ИД, образац, константа |
| foundation | `fkv_error` | модел грешке |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Конфигурација и ИНИ рашчлањивање |
| foundation | `fkv_metrics` | Бројачи и хистограми |
| foundation | `fkv_failpoint` | убризгавање грешке |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | сирово / компресија речника |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | бафер за поравнавање |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Велика вредност и сегмент |
| io | `fkv_io` | Асинхроне ИО особине |
| io | `fkv_io_compio` | цомпио бацкенд |
| io | `fkv_io_mem` | Позадинска меморија |
| io | `fkv_io_sim` | Позадина симулације |
| io | `fkv_object` | Особине складиштења објеката |
| io | `fkv_object_s3` | С3 бацкенд |
| io | `fkv_standby` | копија у стању приправности |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | радна петља и приступ |
| engine | `fkv_store` | животни циклус продавнице и рутирање |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | поновите и поново отворите |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | енд-то-енд бенцхмарк |