# Crate

| шар | Crate | Обов'язки |
|---|---|---|
| foundation | `fkv_types` | ID, шаблон, константа |
| foundation | `fkv_error` | помилкова модель |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Розбір конфігурації та INI |
| foundation | `fkv_metrics` | Лічильники та гістограми |
| foundation | `fkv_failpoint` | ін'єкція помилки |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | стиснення raw / словник |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | вирівняти буфер |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Велика вартість і сегмент |
| io | `fkv_io` | Характеристики асинхронного введення-виведення |
| io | `fkv_io_compio` | сервер компіо |
| io | `fkv_io_mem` | Сервер пам'яті |
| io | `fkv_io_sim` | Сервер моделювання |
| io | `fkv_object` | Ознаки зберігання предметів |
| io | `fkv_object_s3` | Сервер S3 |
| io | `fkv_standby` | резервна копія |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | робочий цикл і доступ |
| engine | `fkv_store` | життєвий цикл магазину та маршрутизація |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | відтворити та знову відкрити |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | наскрізний тест |