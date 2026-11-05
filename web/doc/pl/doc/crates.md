# Crate

| warstwa | Crate | Obowiązki |
|---|---|---|
| foundation | `fkv_types` | Identyfikator, wzór, stała |
| foundation | `fkv_error` | model błędu |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiguracja i parsowanie INI |
| foundation | `fkv_metrics` | Liczniki i histogramy |
| foundation | `fkv_failpoint` | usterka wtrysku |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | Kompresja surowa/słownikowa |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | wyrównaj bufor |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Duża wartość i segment |
| io | `fkv_io` | Asynchroniczne cechy IO |
| io | `fkv_io_compio` | Zaplecze Compio |
| io | `fkv_io_mem` | Zaplecze pamięci |
| io | `fkv_io_sim` | Zaplecze symulacyjne |
| io | `fkv_object` | Cechy przechowywania obiektów |
| io | `fkv_object_s3` | Zaplecze S3 |
| io | `fkv_standby` | kopia rezerwowa |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | pętla pracy i dostęp |
| engine | `fkv_store` | Cykl życia i routing sklepu |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | odtwórz ponownie i otwórz ponownie |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | punkt odniesienia typu end-to-end |