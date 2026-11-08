# 01 总览与分层

## 目标

嵌入式 KV 存储引擎：shard / COW B+tree / manifest 持久化。运行时基于 compio
（thread-per-core）。

## 分层与依赖方向

约定只允许从上往下依赖（同层依赖允许）。依赖方向由
`scripts/ci/check_layers.sh` 在 `./test.sh` 中强制检查：脚本解析各 crate
`Cargo.toml` 的生产 `path` 依赖（不含 `dev-dependencies`），发现从低层指向
高层的边即非零退出。本表是脚本层映射的依据，改层时同步脚本内 `LAYERS`。

| 层 | crate |
|---|---|
| L0 基础 | `fkv_error` `fkv_types` `fkv_codec` `fkv_config` `fkv_metrics` `fkv_failpoint` |
| L1 格式 | `fkv_page` `fkv_compress` `fkv_manifest` |
| L2 内存/树 | `fkv_buf` `fkv_mapping` `fkv_btree` |
| L3 IO | `fkv_io` + `fkv_io_mem` / `fkv_io_compio` / `fkv_io_sim`、`fkv_object` + `fkv_object_s3`、`fkv_standby`、`fkv_large` |
| L4 引擎 | `fkv_api` `fkv_task*`（read/scan/write/reopen/compact/gc/prewarm）`fkv_recovery` `fkv_gc` `fkv_cloud` `fkv_store` `fkv_shard` `fkv_runtime` |
| L5 外观 | `fkv` `fkv_sdk` `fkv_cli` `fkv_testkit` `fkv_bench` |

`crate/test/*`（当前 `fkv_nvme_test`）是只在本地 NVMe / SSD 上运行的隔离
测试 crate，只有 `dev-dependencies`，不参与生产分层，门禁脚本跳过。

约定：

- `fkv_io` 只定义 trait（`PageIo` / `ManifestIo` / `SegmentIo` / `FileIo` /
  `TermIo` / `FixedSegmentIo`），后端在 `fkv_io_mem` / `fkv_io_compio`。
- `fkv_large` 归 L3 IO：它的生产依赖含 `fkv_io`，直接消费 `SegmentIo` /
  `FixedSegmentIo` / `FixedSegmentRead` / `FixedBuffer`，承担大 value 的
  segment 编解码与读写计划，本质是 IO 层组件；列在 L2 会构成 L2 → L3
  倒挂，故与 `fkv_standby` 等并列。
- `fkv_metrics` 是纯 L0 计数（`Op` / `Histogram` / `ShardMetric` /
  `ShardSnapshot`），不依赖 IO；需要指标累计的 IO 装饰器放在使用方
  （当前为 `fkv_shard::metered::MeteredIo`），不要把 `fkv_io` 拉回 L0。
- `fkv_store` 是唯一装配所有后端的地方，泛型 `Store<I>`。
- L5 之外不 re-export 第三方类型。

## 关键对象

| 概念 | Rust |
|---|---|
| 存储引擎 | `Store<I>`（单线程，`&mut self` 写 / `&self` 读） |
| 分区标识 | `TableIdent { tbl_name, partition_id }` |
| 分区状态 | `Store` 内 `table_map: HashMap<TableIdent, TableState>`；每表含 mapper / root / ttl_root / segment_mapper / compression / page_cache / manifest 状态 |
| 运行期进程 | `Runtime`（N 个 `Shard`） |
| 分片 | 1 shard = 1 OS 线程 + 1 compio Runtime + MPSC 队列 |
| 请求 | `fkv_api::{Request, Response}` 纯数据 |

## 命名

- crate 前缀 `fkv_`；C++ 对照 crate 后缀 `_cpp`。
- 盘面文件名由 `fkv_types` 统一生成：`data_<id>_<branch>_<term>`、
  `segment_...`、`manifest_<branch>_<term>`、`CURRENT_TERM_<branch>_<pgid>`。

## 测试与门槛

```bash
./test.sh        # 分层/行数门禁 + cargo nextest（workspace 全量）
cargo +nightly fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
