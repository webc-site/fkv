# Crate

| لایه | Crate | مسئولیت ها |
|---|---|---|
| foundation | `fkv_types` | شناسه، الگو، ثابت |
| foundation | `fkv_error` | مدل خطا |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | پیکربندی و تجزیه INI |
| foundation | `fkv_metrics` | شمارنده ها و هیستوگرام ها |
| foundation | `fkv_failpoint` | تزریق خطا |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | فشرده سازی خام / فرهنگ لغت |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | بافر تراز کردن |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | مقدار و بخش بزرگ |
| io | `fkv_io` | ویژگی های IO ناهمزمان |
| io | `fkv_io_compio` | باطن compio |
| io | `fkv_io_mem` | پشتیبان حافظه |
| io | `fkv_io_sim` | باطن شبیه سازی |
| io | `fkv_object` | ویژگی های ذخیره سازی اشیاء |
| io | `fkv_object_s3` | باطن S3 |
| io | `fkv_standby` | کپی آماده به کار |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | حلقه کار و دسترسی |
| engine | `fkv_store` | چرخه حیات ذخیره و مسیریابی |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | دوباره پخش کنید و دوباره باز کنید |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | معیار انتها به انتها |