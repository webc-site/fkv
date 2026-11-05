# Crate

| lut-od | Crate | Mga responsibilidad |
|---|---|---|
| foundation | `fkv_types` | ID, sumbanan, kanunay |
| foundation | `fkv_error` | sayop nga modelo |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuration ug INI parsing |
| foundation | `fkv_metrics` | Mga Counter ug Histograms |
| foundation | `fkv_failpoint` | sayop nga injection |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | hilaw / diksyonaryo compression |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | align buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Dako nga kantidad ug bahin |
| io | `fkv_io` | Asynchronous nga mga kinaiya sa IO |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Backend sa memorya |
| io | `fkv_io_sim` | Simulation backend |
| io | `fkv_object` | Mga kinaiya sa pagtipig sa butang |
| io | `fkv_object_s3` | S3 nga backend |
| io | `fkv_standby` | standby nga kopya |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | work loop ug access |
| engine | `fkv_store` | store life cycle ug routing |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | replay ug abli pag-usab |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end nga benchmark |