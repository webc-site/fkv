# Crate

| capa | Crate | Responsabilidades |
|---|---|---|
| foundation | `fkv_types` | ID, patrón, constante |
| foundation | `fkv_error` | modelo de error |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuración y análisis INI |
| foundation | `fkv_metrics` | Contadores e histogramas |
| foundation | `fkv_failpoint` | inyección de falla |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | compresión sin procesar/diccionario |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | alinear el búfer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Gran valor y segmento |
| io | `fkv_io` | Rasgos de IO asincrónicos |
| io | `fkv_io_compio` | back-end de compio |
| io | `fkv_io_mem` | Fondo de memoria |
| io | `fkv_io_sim` | backend de simulación |
| io | `fkv_object` | Rasgos de almacenamiento de objetos |
| io | `fkv_object_s3` | back-end de S3 |
| io | `fkv_standby` | copia en espera |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | circuito de trabajo y acceso |
| engine | `fkv_store` | almacenar ciclo de vida y enrutamiento |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | reproducir y volver a abrir |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | punto de referencia de extremo a extremo |