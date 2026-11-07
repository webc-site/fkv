# Crate

| lớp | Crate | Trách nhiệm |
|---|---|---|
| foundation | `fkv_types` | ID, mẫu, hằng số |
| foundation | `fkv_error` | mô hình lỗi |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | Cấu hình và phân tích cú pháp INI |
| foundation | `fkv_metrics` | Bộ đếm và biểu đồ |
| foundation | `fkv_failpoint` | tiêm lỗi |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | nén thô/từ điển |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | căn chỉnh bộ đệm |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | Giá trị lớn và phân khúc |
| io | `fkv_io` | Đặc điểm IO không đồng bộ |
| io | `fkv_io_compio` | phụ trợ compio |
| io | `fkv_io_mem` | Phần phụ trợ bộ nhớ |
| io | `fkv_io_sim` | Phần phụ trợ mô phỏng |
| io | `fkv_object` | Đặc điểm lưu trữ đối tượng |
| io | `fkv_object_s3` | Phụ trợ S3 |
| io | `fkv_standby` | bản sao dự phòng |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | vòng lặp công việc và truy cập |
| engine | `fkv_store` | lưu trữ vòng đời và định tuyến |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | phát lại và mở lại |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | điểm chuẩn từ đầu đến cuối |