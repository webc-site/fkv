# Crate

| plast | Crate | Odgovornosti |
|---|---|---|
| foundation | `fkv_types` | ID, vzorec, konstanta |
| foundation | `fkv_error` | model napake |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Razčlenjevanje konfiguracije in INI |
| foundation | `fkv_metrics` | Števci in histogrami |
| foundation | `fkv_failpoint` | vbrizgavanje napak |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | surovo / slovarsko stiskanje |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | medpomnilnik za poravnavo |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Velika vrednost in segment |
| io | `fkv_io` | Lastnosti asinhronega IO |
| io | `fkv_io_compio` | zaledje compio |
| io | `fkv_io_mem` | Zaledje pomnilnika |
| io | `fkv_io_sim` | Simulacijsko zaledje |
| io | `fkv_object` | Lastnosti shranjevanja predmetov |
| io | `fkv_object_s3` | Zaledje S3 |
| io | `fkv_standby` | kopija v pripravljenosti |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | delovna zanka in dostop |
| engine | `fkv_store` | življenjski cikel in usmerjanje shranjevanja |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ponovno predvajanje in ponovno odpiranje |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | merilo uspešnosti od konca do konca |