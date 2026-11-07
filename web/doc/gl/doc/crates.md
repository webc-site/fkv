# Crate

| capa | Crate | Responsabilidades |
|---|---|---|
| foundation | `fkv_types` | ID, patrón, constante |
| foundation | `fkv_error` | modelo de erro |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuración e análise INI |
| foundation | `fkv_metrics` | Contadores e histogramas |
| foundation | `fkv_failpoint` | inxección de falla |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | compresión bruta / dicionario |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | aliñar o buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Gran valor e segmento |
| io | `fkv_io` | Características de E/S asíncronas |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Backend de memoria |
| io | `fkv_io_sim` | Backend de simulación |
| io | `fkv_object` | Trazos de almacenamento de obxectos |
| io | `fkv_object_s3` | Backend S3 |
| io | `fkv_standby` | copia en espera |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ciclo de traballo e acceso |
| engine | `fkv_store` | ciclo de vida da tenda e enrutamento |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | repetir e abrir de novo |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | punto de referencia de extremo a extremo |