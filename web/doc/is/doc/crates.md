# Crate

| lag | Crate | Ábyrgð |
|---|---|---|
| foundation | `fkv_types` | ID, mynstur, fasti |
| foundation | `fkv_error` | villu líkan |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Stillingar og INI þáttun |
| foundation | `fkv_metrics` | Teljarar og vefrit |
| foundation | `fkv_failpoint` | bilunarsprautun |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | hrá / orðabókarþjöppun |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | samræma biðminni |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Mikið gildi og hluti |
| io | `fkv_io` | Ósamstilltir IO eiginleikar |
| io | `fkv_io_compio` | compio bakendi |
| io | `fkv_io_mem` | Minni bakendi |
| io | `fkv_io_sim` | Bakendi eftirlíkingar |
| io | `fkv_object` | Hlutageymslueiginleikar |
| io | `fkv_object_s3` | S3 bakendi |
| io | `fkv_standby` | biðstöðu afrit |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | vinnulykkja og aðgangur |
| engine | `fkv_store` | geymsluferli og leið |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | spila aftur og opna aftur |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | viðmið frá enda til enda |