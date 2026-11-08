# Crate

| 层 | Crate | 职责 |
|---|---|---|
| foundation | `fkv_types` | ID、模式、常量 |
| foundation | `fkv_error` | 错误模型 |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | 配置与 INI 解析 |
| foundation | `fkv_metrics` | 计数器与直方图 |
| foundation | `fkv_failpoint` | 故障注入 |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | raw / 字典压缩 |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | 对齐 buffer |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | 大 value 与 segment |
| io | `fkv_io` | 异步 IO trait |
| io | `fkv_io_compio` | compio 后端 |
| io | `fkv_io_mem` | 内存后端 |
| io | `fkv_io_sim` | 模拟后端 |
| io | `fkv_object` | 对象存储 trait |
| io | `fkv_object_s3` | S3 后端 |
| io | `fkv_standby` | standby 复制 |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | work loop 与准入 |
| engine | `fkv_store` | store 生命周期与路由 |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | replay 与 reopen |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | 端到端 benchmark |
