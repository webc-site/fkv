# Crate

| слой | Crate | Отговорности |
|---|---|---|
| foundation | `fkv_types` | ID, модел, константа |
| foundation | `fkv_error` | модел на грешка |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Конфигурация и INI анализ |
| foundation | `fkv_metrics` | Броячи и хистограми |
| foundation | `fkv_failpoint` | инжектиране на грешка |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | сурова / речникова компресия |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | буфер за подравняване |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Голяма стойност и сегмент |
| io | `fkv_io` | Асинхронни IO характеристики |
| io | `fkv_io_compio` | бекенд на compio |
| io | `fkv_io_mem` | Бекенд на паметта |
| io | `fkv_io_sim` | Бекенд за симулация |
| io | `fkv_object` | Характеристики на съхранение на обекти |
| io | `fkv_object_s3` | S3 бекенд |
| io | `fkv_standby` | копие в режим на готовност |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | работен цикъл и достъп |
| engine | `fkv_store` | жизнен цикъл на магазина и маршрутизиране |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | повторете и отворете отново |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | бенчмарк от край до край |