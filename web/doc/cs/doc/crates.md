# Crate

| vrstva | Crate | Odpovědnosti |
|---|---|---|
| foundation | `fkv_types` | ID, vzor, ​​konstanta |
| foundation | `fkv_error` | chybový model |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurace a INI analýza |
| foundation | `fkv_metrics` | Čítače a histogramy |
| foundation | `fkv_failpoint` | poruchové vstřikování |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / slovníková komprese |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | zarovnat vyrovnávací paměť |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Velká hodnota a segment |
| io | `fkv_io` | Asynchronní IO vlastnosti |
| io | `fkv_io_compio` | compo backend |
| io | `fkv_io_mem` | Backend paměti |
| io | `fkv_io_sim` | Simulační backend |
| io | `fkv_object` | Vlastnosti ukládání objektů |
| io | `fkv_object_s3` | Backend S3 |
| io | `fkv_standby` | pohotovostní kopie |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | pracovní smyčka a přístup |
| engine | `fkv_store` | životní cyklus úložiště a směrování |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | přehrát a znovu otevřít |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end benchmark |