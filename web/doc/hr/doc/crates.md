# Crate

| sloj | Crate | Odgovornosti |
|---|---|---|
| foundation | `fkv_types` | ID, uzorak, konstanta |
| foundation | `fkv_error` | model pogreške |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguracija i INI parsiranje |
| foundation | `fkv_metrics` | Brojači i histogrami |
| foundation | `fkv_failpoint` | fault injection |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | sirovo / kompresija rječnika |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | uskladiti međuspremnik |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Velika vrijednost i segment |
| io | `fkv_io` | Asinkrone IO osobine |
| io | `fkv_io_compio` | compio backend |
| io | `fkv_io_mem` | Pozadina memorije |
| io | `fkv_io_sim` | Pozadina simulacije |
| io | `fkv_object` | Svojstva skladištenja predmeta |
| io | `fkv_object_s3` | S3 pozadina |
| io | `fkv_standby` | kopija u stanju čekanja |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | radna petlja i pristup |
| engine | `fkv_store` | životni ciklus i usmjeravanje pohrane |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ponoviti i ponovno otvoriti |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | end-to-end benchmark |