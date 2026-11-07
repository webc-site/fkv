# Crate

| স্তৰ | Crate | দায়িত্ব |
|---|---|---|
| foundation | `fkv_types` | আইডি, আৰ্হি, ধ্ৰুৱক |
| foundation | `fkv_error` | ভুল আৰ্হি |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | বিন্যাস আৰু INI বিশ্লেষণ |
| foundation | `fkv_metrics` | কাউণ্টাৰ আৰু হিষ্টোগ্ৰাম |
| foundation | `fkv_failpoint` | ফল্ট ইনজেকচন |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | কেঁচা / অভিধান সংকোচন |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | বাফাৰ প্ৰান্তিককৰণ কৰক |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | বৃহৎ মূল্য আৰু খণ্ড |
| io | `fkv_io` | এচিনক্ৰ'নাছ IO বৈশিষ্ট্যসমূহ |
| io | `fkv_io_compio` | কম্পিঅ' বেকএণ্ড |
| io | `fkv_io_mem` | মেমৰি বেকএণ্ড |
| io | `fkv_io_sim` | চিমুলেচন বেকএণ্ড |
| io | `fkv_object` | বস্তু সংৰক্ষণৰ বৈশিষ্ট্য |
| io | `fkv_object_s3` | S3 বেকএণ্ড |
| io | `fkv_standby` | ষ্টেণ্ডবাই কপি |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | ৱৰ্ক লুপ আৰু এক্সেছ |
| engine | `fkv_store` | ষ্ট'ৰ জীৱনচক্ৰ আৰু ৰুটিং |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | পুনৰায় খেলক আৰু পুনৰ খোলক |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | শেষৰ পৰা শেষলৈ বেঞ্চমাৰ্ক |