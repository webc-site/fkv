# Crate

| Layer | Crate | Responsibilities |
|---|---|---|
| foundation | `fkv_types` | ID, pattern, constant |
| foundation | `fkv_error` | Error Model |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Configuration and INI parsing |
| foundation | `fkv_metrics` | Counters and Histograms |
| foundation | `fkv_failpoint` | Fault Injection |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / dictionary compression |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | Align Buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Large value and segment |
| io | `fkv_io` | Asynchronous IO traits |
| io | `fkv_io_compio` | Compio Backend |
| io | `fkv_io_mem` | Memory backend |
| io | `fkv_io_sim` | Simulation backend |
| io | `fkv_object` | Object storage traits |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | Standby Copy |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | Work Loop and Access |
| engine | `fkv_store` | store life cycle and routing |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | Replay and Reopen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | End-to-End Benchmark |