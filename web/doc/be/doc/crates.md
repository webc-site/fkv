# Crate

| пласт | Crate | Абавязкі |
|---|---|---|
| foundation | `fkv_types` | ID, шаблон, канстанта |
| foundation | `fkv_error` | мадэль памылкі |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Разбор канфігурацыі і INI |
| foundation | `fkv_metrics` | Лічыльнікі і гістаграмы |
| foundation | `fkv_failpoint` | ін'екцыя няспраўнасці |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | неапрацаванае / слоўнікавае сцісканне |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | выраўнаваць буфер |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Вялікае значэнне і сегмент |
| io | `fkv_io` | Асінхронныя рысы ўводу-выводу |
| io | `fkv_io_compio` | бэкэнд compio |
| io | `fkv_io_mem` | Бэкэнд памяці |
| io | `fkv_io_sim` | Бэкэнд мадэлявання |
| io | `fkv_object` | Асаблівасці захоўвання прадмета |
| io | `fkv_object_s3` | Бэкэнд S3 |
| io | `fkv_standby` | рэзервовая копія |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | працоўны цыкл і доступ |
| engine | `fkv_store` | жыццёвы цыкл захоўвання і маршрутызацыя |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | прайграць і зноў адкрыць |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | скразны тэст |