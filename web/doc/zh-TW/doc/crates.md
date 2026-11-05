# Crate

| 層 | Crate | 職責 |
|---|---|---|
| foundation | `fkv_types` | ID、模式、常量 |
| foundation | `fkv_error` | 錯誤模型 |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | 配置與 INI 解析 |
| foundation | `fkv_metrics` | 計數器與直方圖 |
| foundation | `fkv_failpoint` | 故障注入 |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / 字典壓縮 |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | 對齊 buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | 大 value 與 segment |
| io | `fkv_io` | 異步 IO trait |
| io | `fkv_io_compio` | compio 後端 |
| io | `fkv_io_mem` | 記憶體後端 |
| io | `fkv_io_sim` | 模擬後端 |
| io | `fkv_object` | 物件存儲 trait |
| io | `fkv_object_s3` | S3 後端 |
| io | `fkv_standby` | standby 複製 |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | work loop 與準入 |
| engine | `fkv_store` | store 生命周期與路由 |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | replay 與 reopen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | 端到端 benchmark |