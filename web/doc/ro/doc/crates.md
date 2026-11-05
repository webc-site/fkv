# Crate

| strat | Crate | Responsabilitati |
|---|---|---|
| foundation | `fkv_types` | ID, model, constantă |
| foundation | `fkv_error` | model de eroare |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configurare și analiza INI |
| foundation | `fkv_metrics` | Contoare și histograme |
| foundation | `fkv_failpoint` | injectie defect |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | compresie brută / dicționar |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | aliniați tamponul |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Valoare mare și segment |
| io | `fkv_io` | Trăsături IO asincrone |
| io | `fkv_io_compio` | backend compio |
| io | `fkv_io_mem` | Backend de memorie |
| io | `fkv_io_sim` | Backend de simulare |
| io | `fkv_object` | Trăsături de stocare a obiectelor |
| io | `fkv_object_s3` | Backend S3 |
| io | `fkv_standby` | copie de așteptare |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | buclă de lucru și acces |
| engine | `fkv_store` | ciclul de viață al magazinului și rutarea |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | reluați și redeschideți |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | benchmark end-to-end |