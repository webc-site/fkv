# Crate

| strato | Crate | Responsabilità |
|---|---|---|
| foundation | `fkv_types` | ID, modello, costante |
| foundation | `fkv_error` | modello di errore |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configurazione e analisi INI |
| foundation | `fkv_metrics` | Contatori e istogrammi |
| foundation | `fkv_failpoint` | iniezione di guasti |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | compressione raw/dizionario |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | allineare il buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Grande valore e segmento |
| io | `fkv_io` | Tratti IO asincroni |
| io | `fkv_io_compio` | backend compilativo |
| io | `fkv_io_mem` | Backend di memoria |
| io | `fkv_io_sim` | Backend di simulazione |
| io | `fkv_object` | Caratteristiche di archiviazione degli oggetti |
| io | `fkv_object_s3` | Back-end S3 |
| io | `fkv_standby` | copia in attesa |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ciclo di lavoro e accesso |
| engine | `fkv_store` | ciclo di vita e routing del negozio |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | riprodurre e riaprire |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | punto di riferimento end-to-end |