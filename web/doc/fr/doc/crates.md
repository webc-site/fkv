# Crate

| couche | Crate | Responsabilités |
|---|---|---|
| foundation | `fkv_types` | ID, modèle, constante |
| foundation | `fkv_error` | modèle d'erreur |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuration et analyse INI |
| foundation | `fkv_metrics` | Compteurs et histogrammes |
| foundation | `fkv_failpoint` | injection de défauts |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | compression brute / dictionnaire |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | aligner le tampon |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Grande valeur et segment |
| io | `fkv_io` | Caractéristiques d'E/S asynchrones |
| io | `fkv_io_compio` | back-end Compio |
| io | `fkv_io_mem` | Moteur de mémoire |
| io | `fkv_io_sim` | Moteur de simulation |
| io | `fkv_object` | Caractéristiques du stockage d'objets |
| io | `fkv_object_s3` | Moteur S3 |
| io | `fkv_standby` | copie en attente |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | boucle de travail et accès |
| engine | `fkv_store` | cycle de vie et routage du magasin |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | rejouer et rouvrir |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | référence de bout en bout |