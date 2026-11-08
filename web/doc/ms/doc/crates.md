# Crate

| lapisan | Crate | Tanggungjawab |
|---|---|---|
| foundation | `fkv_types` | ID, corak, pemalar |
| foundation | `fkv_error` | model ralat |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurasi dan penghuraian INI |
| foundation | `fkv_metrics` | Pembilang dan Histogram |
| foundation | `fkv_failpoint` | suntikan kesalahan |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | pemampatan mentah / kamus |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | menyelaraskan penimbal |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Nilai dan segmen yang besar |
| io | `fkv_io` | Ciri IO tak segerak |
| io | `fkv_io_compio` | bahagian belakang kompilasi |
| io | `fkv_io_mem` | Bahagian belakang memori |
| io | `fkv_io_sim` | Bahagian belakang simulasi |
| io | `fkv_object` | Ciri penyimpanan objek |
| io | `fkv_object_s3` | Bahagian belakang S3 |
| io | `fkv_standby` | salinan siap sedia |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | gelung kerja dan akses |
| engine | `fkv_store` | kitaran hayat kedai dan penghalaan |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | main semula dan buka semula |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | penanda aras hujung ke hujung |