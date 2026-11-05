# Crate

| थर | Crate | जबाबदाऱ्या |
|---|---|---|
| foundation | `fkv_types` | आयडी, नमुना, स्थिर |
| foundation | `fkv_error` | त्रुटी मॉडेल |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | कॉन्फिगरेशन आणि INI पार्सिंग |
| foundation | `fkv_metrics` | काउंटर आणि हिस्टोग्राम |
| foundation | `fkv_failpoint` | दोष इंजेक्शन |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | रॉ / डिक्शनरी कॉम्प्रेशन |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | बफर संरेखित करा |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | मोठे मूल्य आणि विभाग |
| io | `fkv_io` | असिंक्रोनस IO वैशिष्ट्ये |
| io | `fkv_io_compio` | compio बॅकएंड |
| io | `fkv_io_mem` | मेमरी बॅकएंड |
| io | `fkv_io_sim` | सिम्युलेशन बॅकएंड |
| io | `fkv_object` | ऑब्जेक्ट स्टोरेज वैशिष्ट्ये |
| io | `fkv_object_s3` | S3 बॅकएंड |
| io | `fkv_standby` | स्टँडबाय प्रत |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | कार्य लूप आणि प्रवेश |
| engine | `fkv_store` | जीवन चक्र आणि मार्ग संचयित करा |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | पुन्हा खेळा आणि पुन्हा उघडा |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | एंड-टू-एंड बेंचमार्क |