# Crate

| ಪದರ | Crate | ಜವಾಬ್ದಾರಿಗಳು |
|---|---|---|
| foundation | `fkv_types` | ID, ಮಾದರಿ, ಸ್ಥಿರ |
| foundation | `fkv_error` | ದೋಷ ಮಾದರಿ |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | ಕಾನ್ಫಿಗರೇಶನ್ ಮತ್ತು INI ಪಾರ್ಸಿಂಗ್ |
| foundation | `fkv_metrics` | ಕೌಂಟರ್‌ಗಳು ಮತ್ತು ಹಿಸ್ಟೋಗ್ರಾಮ್‌ಗಳು |
| foundation | `fkv_failpoint` | ತಪ್ಪು ಇಂಜೆಕ್ಷನ್ |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | ಕಚ್ಚಾ / ನಿಘಂಟು ಸಂಕೋಚನ |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | ಬಫರ್ ಅನ್ನು ಜೋಡಿಸಿ |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | ದೊಡ್ಡ ಮೌಲ್ಯ ಮತ್ತು ವಿಭಾಗ |
| io | `fkv_io` | ಅಸಮಕಾಲಿಕ IO ಗುಣಲಕ್ಷಣಗಳು |
| io | `fkv_io_compio` | ಸಂಯೋಜನೆ ಬ್ಯಾಕೆಂಡ್ |
| io | `fkv_io_mem` | ಮೆಮೊರಿ ಬ್ಯಾಕೆಂಡ್ |
| io | `fkv_io_sim` | ಸಿಮ್ಯುಲೇಶನ್ ಬ್ಯಾಕೆಂಡ್ |
| io | `fkv_object` | ವಸ್ತು ಸಂಗ್ರಹಣೆಯ ಲಕ್ಷಣಗಳು |
| io | `fkv_object_s3` | S3 ಬ್ಯಾಕೆಂಡ್ |
| io | `fkv_standby` | ಸ್ಟ್ಯಾಂಡ್ಬೈ ನಕಲು |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ಕೆಲಸದ ಲೂಪ್ ಮತ್ತು ಪ್ರವೇಶ |
| engine | `fkv_store` | ಜೀವನ ಚಕ್ರ ಮತ್ತು ರೂಟಿಂಗ್ ಅನ್ನು ಸಂಗ್ರಹಿಸಿ |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | ಮರುಪಂದ್ಯ ಮತ್ತು ಪುನಃ ತೆರೆಯಿರಿ |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | ಅಂತ್ಯದಿಂದ ಅಂತ್ಯದ ಮಾನದಂಡ |