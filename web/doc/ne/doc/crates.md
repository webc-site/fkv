# Crate

| तह | Crate | जिम्मेवारीहरू |
|---|---|---|
| foundation | `fkv_types` | आईडी, ढाँचा, स्थिर |
| foundation | `fkv_error` | त्रुटि मोडेल |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | कन्फिगरेसन र INI पार्सिङ |
| foundation | `fkv_metrics` | काउन्टर र हिस्टोग्राम |
| foundation | `fkv_failpoint` | दोष इंजेक्शन |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | कच्चा / शब्दकोश सङ्कुचन |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | पङ्क्तिबद्ध बफर |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | ठूलो मान र खण्ड |
| io | `fkv_io` | एसिन्क्रोनस IO विशेषताहरू |
| io | `fkv_io_compio` | कम्पियो ब्याकइन्ड |
| io | `fkv_io_mem` | मेमोरी ब्याकइन्ड |
| io | `fkv_io_sim` | सिमुलेशन ब्याकइन्ड |
| io | `fkv_object` | वस्तु भण्डारण विशेषताहरू |
| io | `fkv_object_s3` | S3 ब्याकइन्ड |
| io | `fkv_standby` | स्ट्यान्डबाइ प्रतिलिपि |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | काम लूप र पहुँच |
| engine | `fkv_store` | जीवन चक्र र मार्ग भण्डारण गर्नुहोस् |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | पुन: खेल्नुहोस् र पुन: खोल्नुहोस् |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | अन्त-देखि-अन्त बेन्चमार्क |