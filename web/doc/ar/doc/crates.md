# Crate

| طبقة | Crate | المسؤوليات |
|---|---|---|
| foundation | `fkv_types` | معرف، نمط، ثابت |
| foundation | `fkv_error` | نموذج الخطأ |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | التكوين وتحليل INI |
| foundation | `fkv_metrics` | العدادات والرسوم البيانية |
| foundation | `fkv_failpoint` | حقن خطأ |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | ضغط الخام / القاموس |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | محاذاة المخزن المؤقت |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | قيمة كبيرة وقطاع |
| io | `fkv_io` | سمات IO غير المتزامنة |
| io | `fkv_io_compio` | الخلفية كومبيو |
| io | `fkv_io_mem` | الواجهة الخلفية للذاكرة |
| io | `fkv_io_sim` | محاكاة الخلفية |
| io | `fkv_object` | سمات تخزين الكائنات |
| io | `fkv_object_s3` | S3 الخلفية |
| io | `fkv_standby` | نسخة الاستعداد |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | حلقة العمل والوصول |
| engine | `fkv_store` | تخزين دورة الحياة والتوجيه |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | إعادة التشغيل وإعادة فتحه |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | نهاية إلى نهاية المعيار |