# Crate

| lager | Crate | Ansvar |
|---|---|---|
| foundation | `fkv_types` | ID, mönster, konstant |
| foundation | `fkv_error` | felmodell |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguration och INI-analys |
| foundation | `fkv_metrics` | Räknare och histogram |
| foundation | `fkv_failpoint` | felinsprutning |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | rå-/ordbokskomprimering |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | justera bufferten |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Stort värde och segment |
| io | `fkv_io` | Asynkrona IO-egenskaper |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Minnesbackend |
| io | `fkv_io_sim` | Simuleringsbackend |
| io | `fkv_object` | Objektlagringsegenskaper |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | standby-kopia |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | arbetsslinga och åtkomst |
| engine | `fkv_store` | lagra livscykel och routing |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | spela upp och öppna igen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end benchmark |