# Crate

| haenen | Crate | Cyfrifoldebau |
|---|---|---|
| foundation | `fkv_types` | ID, patrwm, cyson |
| foundation | `fkv_error` | model gwall |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Ffurfweddiad a dosrannu INI |
| foundation | `fkv_metrics` | Cownteri a Histogramau |
| foundation | `fkv_failpoint` | pigiad fai |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | cywasgiad amrwd/geiriadur |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | alinio byffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Gwerth mawr a segment |
| io | `fkv_io` | Nodweddion IO asyncronig |
| io | `fkv_io_compio` | backend compio |
| io | `fkv_io_mem` | Backend cof |
| io | `fkv_io_sim` | Backend efelychiad |
| io | `fkv_object` | Nodweddion storio gwrthrychau |
| io | `fkv_object_s3` | S3 cefn |
| io | `fkv_standby` | copi wrth gefn |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | dolen waith a mynediad |
| engine | `fkv_store` | cylch bywyd storio a llwybro |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ailchwarae ac ailagor |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | meincnod pen-i-ben |