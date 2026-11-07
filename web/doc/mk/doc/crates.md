# Crate

| слој | Crate | Одговорности |
|---|---|---|
| foundation | `fkv_types` | ИД, шема, константа |
| foundation | `fkv_error` | модел на грешка |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Конфигурација и парсирање на INI |
| foundation | `fkv_metrics` | Бројачи и хистограми |
| foundation | `fkv_failpoint` | неисправна инјекција |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | сурова / компресија на речник |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | порамни бафер |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Голема вредност и сегмент |
| io | `fkv_io` | Асинхрони IO особини |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Заднина на меморијата |
| io | `fkv_io_sim` | Симулациски заден дел |
| io | `fkv_object` | Карактеристики за складирање на предмети |
| io | `fkv_object_s3` | S3 заднина |
| io | `fkv_standby` | копија во мирување |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | работна јамка и пристап |
| engine | `fkv_store` | животниот циклус на складирање и рутирање |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | репродуцирајте и повторно отворете |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | репер од крај до крај |