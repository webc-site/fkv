# Crate

| слой | Crate | Обязанности |
|---|---|---|
| foundation | `fkv_types` | ID, шаблон, константа |
| foundation | `fkv_error` | модель ошибки |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Конфигурация и разбор INI |
| foundation | `fkv_metrics` | Счетчики и гистограммы |
| foundation | `fkv_failpoint` | внесение неисправностей |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | необработанное/словарное сжатие |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | выровнять буфер |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Большая стоимость и сегмент |
| io | `fkv_io` | Особенности асинхронного ввода-вывода |
| io | `fkv_io_compio` | серверная часть компиляции |
| io | `fkv_io_mem` | Серверная часть памяти |
| io | `fkv_io_sim` | Серверная часть моделирования |
| io | `fkv_object` | Особенности хранения объектов |
| io | `fkv_object_s3` | Серверная часть S3 |
| io | `fkv_standby` | резервная копия |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | рабочий цикл и доступ |
| engine | `fkv_store` | Жизненный цикл хранилища и маршрутизация |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | переиграть и открыть заново |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | сквозной тест |