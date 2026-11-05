# Crate

| Schicht | Crate | Verantwortlichkeiten |
|---|---|---|
| foundation | `fkv_types` | ID, Muster, Konstante |
| foundation | `fkv_error` | Fehlermodell |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguration und INI-Parsing |
| foundation | `fkv_metrics` | Zähler und Histogramme |
| foundation | `fkv_failpoint` | Fehlerinjektion |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | Roh-/Wörterbuchkomprimierung |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | Puffer ausrichten |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Großer Wert und Segment |
| io | `fkv_io` | Asynchrone IO-Merkmale |
| io | `fkv_io_compio` | Compio-Backend |
| io | `fkv_io_mem` | Speicher-Backend |
| io | `fkv_io_sim` | Simulations-Backend |
| io | `fkv_object` | Eigenschaften der Objektspeicherung |
| io | `fkv_object_s3` | S3-Backend |
| io | `fkv_standby` | Standby-Kopie |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | Arbeitsschleife und Zugang |
| engine | `fkv_store` | Speicherlebenszyklus und Routing |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | erneut abspielen und erneut öffnen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | End-to-End-Benchmark |