# Crate

| sluoksnis | Crate | Pareigos |
|---|---|---|
| foundation | `fkv_types` | ID, modelis, konstanta |
| foundation | `fkv_error` | klaidos modelis |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigūracija ir INI analizavimas |
| foundation | `fkv_metrics` | Skaitikliai ir histogramos |
| foundation | `fkv_failpoint` | gedimo įpurškimas |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | neapdorotas / žodyno suspaudimas |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | suderinti buferį |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Didelė vertė ir segmentas |
| io | `fkv_io` | Asinchroninės IO savybės |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Atminties užpakalinė programa |
| io | `fkv_io_sim` | Modeliavimo užpakalinė programa |
| io | `fkv_object` | Objektų saugojimo savybės |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | budėjimo režimo kopija |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | darbo kilpa ir prieiga |
| engine | `fkv_store` | parduotuvės gyvavimo ciklas ir maršrutas |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | paleisti ir vėl atidaryti |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | galutinis etalonas |