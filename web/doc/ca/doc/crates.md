# Crate

| capa | Crate | Responsabilitats |
|---|---|---|
| foundation | `fkv_types` | ID, patró, constant |
| foundation | `fkv_error` | model d'error |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuració i anàlisi INI |
| foundation | `fkv_metrics` | Comptadors i histogrames |
| foundation | `fkv_failpoint` | injecció de falla |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | compressió en brut / diccionari |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | alinear el buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Gran valor i segment |
| io | `fkv_io` | Trets d'E/S asíncrons |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Backend de memòria |
| io | `fkv_io_sim` | Backend de simulació |
| io | `fkv_object` | Característiques d'emmagatzematge d'objectes |
| io | `fkv_object_s3` | Backend S3 |
| io | `fkv_standby` | còpia en espera |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | bucle de treball i accés |
| engine | `fkv_store` | cicle de vida de la botiga i encaminament |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | repetir i tornar a obrir |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | referència d'extrem a extrem |