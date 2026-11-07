# Crate

| kerros | Crate | Vastuut |
|---|---|---|
| foundation | `fkv_types` | ID, malli, vakio |
| foundation | `fkv_error` | virhemalli |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Konfigurointi ja INI-jäsennys |
| foundation | `fkv_metrics` | Laskurit ja histogrammit |
| foundation | `fkv_failpoint` | vian ruiskutus |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raaka / sanakirjan pakkaus |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | kohdista puskuri |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Suuri arvo ja segmentti |
| io | `fkv_io` | Asynkroniset IO-ominaisuudet |
| io | `fkv_io_compio` | compio tausta |
| io | `fkv_io_mem` | Muistin tausta |
| io | `fkv_io_sim` | Simuloinnin taustaohjelma |
| io | `fkv_object` | Esineiden säilytysominaisuudet |
| io | `fkv_object_s3` | S3 tausta |
| io | `fkv_standby` | valmiustilakopio |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | työsilmukka ja pääsy |
| engine | `fkv_store` | myymälän elinkaari ja reititys |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | toista ja avaa uudelleen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | päästä päähän -benchmark |