# Crate

| परत | Crate | जिम्मेदारियों |
|---|---|---|
| foundation | `fkv_types` | आईडी, पैटर्न, स्थिरांक |
| foundation | `fkv_error` | त्रुटि मॉडल |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | कॉन्फ़िगरेशन और INI पार्सिंग |
| foundation | `fkv_metrics` | काउंटर और हिस्टोग्राम |
| foundation | `fkv_failpoint` | दोष इंजेक्शन |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | कच्चा/शब्दकोश संपीड़न |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | बफ़र संरेखित करें |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | बड़ा मूल्य और खंड |
| io | `fkv_io` | अतुल्यकालिक IO लक्षण |
| io | `fkv_io_compio` | कंपियो बैकएंड |
| io | `fkv_io_mem` | मेमोरी बैकएंड |
| io | `fkv_io_sim` | सिमुलेशन बैकएंड |
| io | `fkv_object` | वस्तु भंडारण लक्षण |
| io | `fkv_object_s3` | S3 बैकएंड |
| io | `fkv_standby` | स्टैंडबाय कॉपी |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | कार्य लूप और पहुंच |
| engine | `fkv_store` | स्टोर जीवन चक्र और रूटिंग |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | पुनः चलाएँ और पुनः खोलें |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | एंड-टू-एंड बेंचमार्क |