# Crate

| katman | Crate | Sorumluluklar |
|---|---|---|
| foundation | `fkv_types` | Kimlik, desen, sabit |
| foundation | `fkv_error` | hata modeli |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Yapılandırma ve INI ayrıştırma |
| foundation | `fkv_metrics` | Sayaçlar ve Histogramlar |
| foundation | `fkv_failpoint` | arıza enjeksiyonu |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | ham / sözlük sıkıştırması |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | arabelleği hizala |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Büyük değer ve segment |
| io | `fkv_io` | Eşzamansız GÇ özellikleri |
| io | `fkv_io_compio` | kompozisyon arka ucu |
| io | `fkv_io_mem` | Bellek arka ucu |
| io | `fkv_io_sim` | Simülasyon arka ucu |
| io | `fkv_object` | Nesne depolama özellikleri |
| io | `fkv_object_s3` | S3 arka ucu |
| io | `fkv_standby` | bekleme kopyası |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | çalışma döngüsü ve erişim |
| engine | `fkv_store` | mağaza yaşam döngüsü ve yönlendirme |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | tekrar oynat ve yeniden aç |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | uçtan uca kıyaslama |