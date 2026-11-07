# Crate

| qat | Crate | Məsuliyyətlər |
|---|---|---|
| foundation | `fkv_types` | ID, naxış, sabit |
| foundation | `fkv_error` | səhv modeli |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfiqurasiya və INI təhlili |
| foundation | `fkv_metrics` | Sayğaclar və Histoqramlar |
| foundation | `fkv_failpoint` | səhv enjeksiyon |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | xam / lüğət sıxılması |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | buferi hizalayın |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Böyük dəyər və seqment |
| io | `fkv_io` | Asinxron IO xüsusiyyətləri |
| io | `fkv_io_compio` | kompio backend |
| io | `fkv_io_mem` | Yaddaş arxa ucu |
| io | `fkv_io_sim` | Simulyasiya arxa ucu |
| io | `fkv_object` | Obyekt saxlama xüsusiyyətləri |
| io | `fkv_object_s3` | S3 arxa uç |
| io | `fkv_standby` | gözləmə nüsxəsi |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | iş dövrəsi və giriş |
| engine | `fkv_store` | mağazanın həyat dövrü və marşrutlaşdırma |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | təkrar oxuyun və açın |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | uç-to-end benchmark |