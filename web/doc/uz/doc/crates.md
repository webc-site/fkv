# Crate

| qatlam | Crate | Mas'uliyat |
|---|---|---|
| foundation | `fkv_types` | ID, naqsh, doimiy |
| foundation | `fkv_error` | xato modeli |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguratsiya va INI tahlili |
| foundation | `fkv_metrics` | Hisoblagichlar va gistogrammalar |
| foundation | `fkv_failpoint` | noto'g'ri in'ektsiya |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | xom / lug'atni siqish |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | buferni tekislang |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Katta qiymat va segment |
| io | `fkv_io` | Asinxron IO xususiyatlari |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Xotiraning orqa tomoni |
| io | `fkv_io_sim` | Simulyatsiya backend |
| io | `fkv_object` | Ob'ektni saqlash xususiyatlari |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | kutish nusxasi |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ish davri va kirish |
| engine | `fkv_store` | do'konning hayot aylanishi va marshrutlash |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | takrorlang va qayta oching |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end benchmark |