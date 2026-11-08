# Crate

| அடுக்கு | Crate | பொறுப்புகள் |
|---|---|---|
| foundation | `fkv_types` | ஐடி, பேட்டர்ன், மாறிலி |
| foundation | `fkv_error` | பிழை மாதிரி |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | கட்டமைப்பு மற்றும் INI பாகுபடுத்துதல் |
| foundation | `fkv_metrics` | கவுண்டர்கள் மற்றும் ஹிஸ்டோகிராம்கள் |
| foundation | `fkv_failpoint` | தவறு ஊசி |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | மூல / அகராதி சுருக்கம் |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | align buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | பெரிய மதிப்பு மற்றும் பிரிவு |
| io | `fkv_io` | ஒத்திசைவற்ற IO பண்புகள் |
| io | `fkv_io_compio` | தொகுப்பு பின்தளத்தில் |
| io | `fkv_io_mem` | நினைவக பின்தளம் |
| io | `fkv_io_sim` | உருவகப்படுத்துதல் பின்தளம் |
| io | `fkv_object` | பொருள் சேமிப்பு பண்புகள் |
| io | `fkv_object_s3` | S3 பின்தளம் |
| io | `fkv_standby` | காத்திருப்பு நகல் |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | வேலை வளையம் மற்றும் அணுகல் |
| engine | `fkv_store` | சேமிப்பு வாழ்க்கை சுழற்சி மற்றும் ரூட்டிங் |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | மீண்டும் இயக்கி மீண்டும் திறக்கவும் |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | இறுதி முதல் இறுதி வரையிலான அளவுகோல் |