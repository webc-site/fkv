# Crate

| kiht | Crate | Kohustused |
|---|---|---|
| foundation | `fkv_types` | ID, muster, konstant |
| foundation | `fkv_error` | veamudel |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguratsioon ja INI sõelumine |
| foundation | `fkv_metrics` | Loendurid ja histogrammid |
| foundation | `fkv_failpoint` | vea süstimine |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | toores / sõnastiku tihendamine |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | joondada puhvrit |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Suur väärtus ja segment |
| io | `fkv_io` | Asünkroonsed IO tunnused |
| io | `fkv_io_compio` | kompio taustaprogramm |
| io | `fkv_io_mem` | Mälu taustaprogramm |
| io | `fkv_io_sim` | Simulatsiooni taustaprogramm |
| io | `fkv_object` | Objektide säilitamise omadused |
| io | `fkv_object_s3` | S3 taustaprogramm |
| io | `fkv_standby` | ooterežiimi koopia |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | töösilmus ja juurdepääs |
| engine | `fkv_store` | poe elutsükkel ja marsruutimine |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | taasesitada ja uuesti avada |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | otsast lõpuni etalon |