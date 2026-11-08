# Crate

| স্তর | Crate | দায়িত্ব |
|---|---|---|
| foundation | `fkv_types` | আইডি, প্যাটার্ন, ধ্রুবক |
| foundation | `fkv_error` | ত্রুটি মডেল |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | কনফিগারেশন এবং INI পার্সিং |
| foundation | `fkv_metrics` | কাউন্টার এবং হিস্টোগ্রাম |
| foundation | `fkv_failpoint` | দোষ ইনজেকশন |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | কাঁচা / অভিধান কম্প্রেশন |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | সারিবদ্ধ বাফার |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | বড় মান এবং সেগমেন্ট |
| io | `fkv_io` | অ্যাসিঙ্ক্রোনাস আইও বৈশিষ্ট্য |
| io | `fkv_io_compio` | কম্পিও ব্যাকএন্ড |
| io | `fkv_io_mem` | মেমরি ব্যাকএন্ড |
| io | `fkv_io_sim` | সিমুলেশন ব্যাকএন্ড |
| io | `fkv_object` | অবজেক্ট স্টোরেজ বৈশিষ্ট্য |
| io | `fkv_object_s3` | S3 ব্যাকএন্ড |
| io | `fkv_standby` | স্ট্যান্ডবাই কপি |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | কাজের লুপ এবং অ্যাক্সেস |
| engine | `fkv_store` | সঞ্চয় জীবন চক্র এবং রাউটিং |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | পুনরায় খেলুন এবং পুনরায় খুলুন |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | এন্ড-টু-এন্ড বেঞ্চমার্ক |