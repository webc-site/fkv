# Crate

| қабат | Crate | Жауапкершіліктер |
|---|---|---|
| foundation | `fkv_types` | Идентификатор, үлгі, тұрақты |
| foundation | `fkv_error` | қате үлгісі |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Конфигурация және INI талдауы |
| foundation | `fkv_metrics` | Есептегіштер және гистограммалар |
| foundation | `fkv_failpoint` | ақаулы инъекция |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | шикі/сөздік қысу |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | буферді туралаңыз |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Үлкен мән және сегмент |
| io | `fkv_io` | Асинхронды IO белгілері |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Жад сервері |
| io | `fkv_io_sim` | Модельдеу сервері |
| io | `fkv_object` | Объектіні сақтау қасиеттері |
| io | `fkv_object_s3` | S3 сервері |
| io | `fkv_standby` | күту режиміндегі көшірме |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | жұмыс циклі және қол жеткізу |
| engine | `fkv_store` | сақтаудың өмірлік циклі және маршруттау |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | қайта ойнатыңыз және қайта ашыңыз |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | түпкілікті эталон |