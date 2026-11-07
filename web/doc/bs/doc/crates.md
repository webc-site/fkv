# Crate

| sloj | Crate | Odgovornosti |
|---|---|---|
| foundation | `fkv_types` | ID, obrazac, konstanta |
| foundation | `fkv_error` | model greške |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguracija i INI raščlanjivanje |
| foundation | `fkv_metrics` | Brojači i histogrami |
| foundation | `fkv_failpoint` | ubrizgavanje greške |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / kompresija rječnika |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | align buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Velika vrijednost i segment |
| io | `fkv_io` | Asinhrone IO osobine |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Memorijska pozadina |
| io | `fkv_io_sim` | Pozadina simulacije |
| io | `fkv_object` | Osobine skladištenja objekata |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | kopija u stanju pripravnosti |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | radna petlja i pristup |
| engine | `fkv_store` | životni ciklus skladišta i rutiranje |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ponovi i ponovo otvori |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end benchmark |