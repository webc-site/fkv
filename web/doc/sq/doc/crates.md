# Crate

| shtresë | Crate | Përgjegjësitë |
|---|---|---|
| foundation | `fkv_types` | ID, model, konstante |
| foundation | `fkv_error` | modeli i gabimit |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurimi dhe analizimi INI |
| foundation | `fkv_metrics` | Numëruesit dhe histogramet |
| foundation | `fkv_failpoint` | injeksion gabimi |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | kompresim i papërpunuar / fjalor |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | rreshtoj buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Vlera dhe segmenti i madh |
| io | `fkv_io` | Tiparet asinkrone të IO |
| io | `fkv_io_compio` | backend compio |
| io | `fkv_io_mem` | Mbështetja e kujtesës |
| io | `fkv_io_sim` | Mbështetja e simulimit |
| io | `fkv_object` | Karakteristikat e ruajtjes së objekteve |
| io | `fkv_object_s3` | S3 backend |
| io | `fkv_standby` | kopje gatishmërie |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | cikli i punës dhe aksesi |
| engine | `fkv_store` | cikli jetësor i magazinimit dhe rrugëtimi |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | riluaje dhe rihap |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | pikë referimi nga fundi në fund |