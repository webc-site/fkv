# Crate

| réteg | Crate | Felelősségek |
|---|---|---|
| foundation | `fkv_types` | ID, minta, állandó |
| foundation | `fkv_error` | hibamodell |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguráció és INI elemzés |
| foundation | `fkv_metrics` | Számlálók és hisztogramok |
| foundation | `fkv_failpoint` | hibabefecskendezés |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | nyers / szótár tömörítés |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | igazítsa a puffert |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Nagy érték és szegmens |
| io | `fkv_io` | Aszinkron IO tulajdonságok |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Memória háttér |
| io | `fkv_io_sim` | Szimulációs háttér |
| io | `fkv_object` | Tárgytárolási jellemzők |
| io | `fkv_object_s3` | S3 háttér |
| io | `fkv_standby` | készenléti másolat |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | munkahurok és hozzáférés |
| engine | `fkv_store` | az üzlet életciklusa és útválasztása |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | újrajátszás és újranyitás |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | végpontok közötti benchmark |