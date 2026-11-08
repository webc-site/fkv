# Crate

| շերտ | Crate | Պարտականություններ |
|---|---|---|
| foundation | `fkv_types` | ID, օրինաչափություն, հաստատուն |
| foundation | `fkv_error` | սխալի մոդել |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Կազմաձևում և INI վերլուծություն |
| foundation | `fkv_metrics` | Հաշվիչներ և հիստոգրամներ |
| foundation | `fkv_failpoint` | մեղքի ներարկում |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | հում / բառարան սեղմում |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | հավասարեցնել բուֆերը |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Մեծ արժեք և հատված |
| io | `fkv_io` | Ասինխրոն IO հատկանիշներ |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Հիշողության հետնամաս |
| io | `fkv_io_sim` | Մոդելավորման հետնամաս |
| io | `fkv_object` | Օբյեկտների պահպանման առանձնահատկությունները |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | սպասման պատճեն |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | աշխատանքային հանգույց և մուտք |
| engine | `fkv_store` | պահեստի կյանքի ցիկլը և երթուղին |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | վերարտադրել և նորից բացել |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | վերջից մինչև վերջ չափանիշ |