# Crate

| Layer | Crate | Responsabilitéiten |
|---|---|---|
| foundation | `fkv_types` | ID, Muster, konstant |
| foundation | `fkv_error` | Feeler Modell |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuratioun an INI Parsing |
| foundation | `fkv_metrics` | Zähler und Histogramme |
| foundation | `fkv_failpoint` | Feeler Sprëtz |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / Wierderbuch Kompressioun |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | alignéieren Puffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Grousse Wäert a Segment |
| io | `fkv_io` | Asynchron IO Charaktere |
| io | `fkv_io_compio` | compio Backend |
| io | `fkv_io_mem` | Erënnerung Backend |
| io | `fkv_io_sim` | Simulatioun Backend |
| io | `fkv_object` | Objektlagerungseigenschaften |
| io | `fkv_object_s3` | S3 Backend |
| io | `fkv_standby` | Standby Kopie |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | Aarbecht Loop an Zougang |
| engine | `fkv_store` | Store Liewenszyklus a Routing |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | Replay an Reouverture |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | Enn-zu-Enn Benchmark |