# Crate

| پرت | Crate | ذمہ داریاں |
|---|---|---|
| foundation | `fkv_types` | ID، پیٹرن، مسلسل |
| foundation | `fkv_error` | غلطی کا ماڈل |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | کنفیگریشن اور INI پارسنگ |
| foundation | `fkv_metrics` | کاؤنٹرز اور ہسٹوگرام |
| foundation | `fkv_failpoint` | غلطی انجکشن |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | خام / لغت کمپریشن |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | بفر سیدھ کریں |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | بڑی قدر اور طبقہ |
| io | `fkv_io` | غیر مطابقت پذیر IO خصوصیات |
| io | `fkv_io_compio` | کمپیو پسدید |
| io | `fkv_io_mem` | میموری پسدید |
| io | `fkv_io_sim` | نقلی پسدید |
| io | `fkv_object` | آبجیکٹ اسٹوریج کی خصوصیات |
| io | `fkv_object_s3` | S3 پسدید |
| io | `fkv_standby` | اسٹینڈ بائی کاپی |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | کام لوپ اور رسائی |
| engine | `fkv_store` | لائف سائیکل اور روٹنگ کو اسٹور کریں۔ |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | دوبارہ چلائیں اور دوبارہ کھولیں۔ |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | آخر سے آخر تک بینچ مارک |