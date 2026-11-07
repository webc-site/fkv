# Crate

| ස්ථරය | Crate | වගකීම් |
|---|---|---|
| foundation | `fkv_types` | හැඳුනුම්පත, රටාව, නියත |
| foundation | `fkv_error` | දෝෂ ආකෘතිය |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | වින්‍යාස කිරීම සහ INI විග්‍රහ කිරීම |
| foundation | `fkv_metrics` | කවුන්ටර සහ හිස්ටෝග්රෑම් |
| foundation | `fkv_failpoint` | වැරදි එන්නත් කිරීම |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | අමු / ශබ්ද කෝෂ සම්පීඩනය |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | align buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | විශාල අගය සහ කොටස |
| io | `fkv_io` | අසමමුහුර්ත IO ලක්ෂණ |
| io | `fkv_io_compio` | compio පසුබිම |
| io | `fkv_io_mem` | මතක පසුබිම |
| io | `fkv_io_sim` | අනුකරණ පසුබිම |
| io | `fkv_object` | වස්තු ගබඩා කිරීමේ ලක්ෂණ |
| io | `fkv_object_s3` | S3 පසුබිම |
| io | `fkv_standby` | පොරොත්තු පිටපත |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | වැඩ ලූප් සහ ප්රවේශය |
| engine | `fkv_store` | ගබඩා ජීවන චක්රය සහ මාර්ගගත කිරීම |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | නැවත ධාවනය කර නැවත විවෘත කරන්න |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | අග සිට අග දක්වා මිණුම් ලකුණ |