# 01 总览与分层

## 目标

嵌入式 KV 存储引擎：shard / COW B+tree / manifest 持久化。运行时基于 compio
（thread-per-core）。

## 分层与依赖方向

约定只允许从上往下依赖。**当前没有 `xtask` 或 CI 依赖方向检查**，靠 review
与 `cargo shear`（本地可选）发现倒挂；不要把本表当成已强制执行的门禁。

| 层 | crate |
|---|---|
| L0 基础 | `fkv_error` `fkv_types` `fkv_codec` `fkv_config` `fkv_metrics` `fkv_failpoint` |
| L1 格式 | `fkv_page` `fkv_compress` `fkv_manifest` |
| L2 内存/树 | `fkv_buf` `fkv_mapping` `fkv_btree` `fkv_large` |
| L3 IO | `fkv_io` + `fkv_io_mem` / `fkv_io_compio` / `fkv_io_sim`、`fkv_object` + `fkv_object_s3`、`fkv_standby` |
| L4 引擎 | `fkv_api` `fkv_task*`（read/scan/write/reopen/compact/gc/prewarm）`fkv_recovery` `fkv_gc` `fkv_cloud` `fkv_store` `fkv_shard` `fkv_runtime` |
| L5 外观 | `fkv` `fkv_sdk` `fkv_cli` `fkv_testkit` `fkv_bench` |

约定：

- `fkv_io` 只定义 trait（`PageIo` / `ManifestIo` / `SegmentIo` / `FileIo` /
  `TermIo` / `FixedSegmentIo`），后端在 `fkv_io_mem` / `fkv_io_compio`。
- `fkv_store` 是唯一装配所有后端的地方，泛型 `Store<I>`。
- L5 之外不 re-export 第三方类型。

## 关键对象

| 概念 | Rust |
|---|---|
| 存储引擎 | `Store<I>`（单线程，`&mut self` 写 / `&self` 读） |
| 分区标识 | `TableIdent { tbl_name, partition_id }` |
| 分区状态 | `Store` 内 `table_map: HashMap<TableIdent, TableState>`；每表含 mapper / root / ttl_root / segment_mapper / compression / index_cache / manifest 状态 |
| 运行期进程 | `Runtime`（N 个 `Shard`） |
| 分片 | 1 shard = 1 OS 线程 + 1 compio Runtime + MPSC 队列 |
| 请求 | `fkv_api::{Request, Response}` 纯数据 |

## 命名

- crate 前缀 `fkv_`；C++ 对照 crate 后缀 `_cpp`。
- 盘面文件名由 `fkv_types` 统一生成：`data_<id>_<branch>_<term>`、
  `segment_...`、`manifest_<branch>_<term>`、`CURRENT_TERM_<branch>_<pgid>`。

## 测试与门槛

```bash
./test.sh        # cargo nextest（+ fkv_ledger 单测）
cargo +nightly fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
