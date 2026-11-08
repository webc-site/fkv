# Crate

| שִׁכבָה | Crate | אחריות |
|---|---|---|
| foundation | `fkv_types` | מזהה, דפוס, קבוע |
| foundation | `fkv_error` | מודל שגיאה |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | תצורה וניתוח INI |
| foundation | `fkv_metrics` | מונים והיסטוגרמות |
| foundation | `fkv_failpoint` | הזרקת תקלה |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | דחיסה גולמית/מילון |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | יישר מאגר |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | ערך ופלח גדול |
| io | `fkv_io` | תכונות IO אסינכרוניות |
| io | `fkv_io_compio` | קצה אחורי של compio |
| io | `fkv_io_mem` | אחורי זיכרון |
| io | `fkv_io_sim` | אחורי של סימולציה |
| io | `fkv_object` | תכונות אחסון חפצים |
| io | `fkv_object_s3` | קצה אחורי של S3 |
| io | `fkv_standby` | עותק המתנה |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | לולאת עבודה וגישה |
| engine | `fkv_store` | מחזור החיים של החנות והניתוב |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | לשחק מחדש ולפתוח מחדש |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | אמת מידה מקצה לקצה |