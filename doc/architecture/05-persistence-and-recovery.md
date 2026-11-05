# 05 持久化与恢复

## manifest 是唯一 durability anchor

每个分区（每个 branch）一份 manifest 文件，由两类记录组成：

| 记录 | 内容 |
|---|---|
| snapshot | root、ttl_root、max_fp_id、`dict_bytes`、整份 mapping 值、branch 元数据、segment mapping 与 max_segment_fp_id |
| log | 增量：新的 root / ttl_root + 变化页的 mapping（segment 用 u32 长度前缀的 delta） |

记录格式：`checksum(8B) | root | ttl_root | payload_len | payload`；
尾部不完整的记录在 replay 时被截断（torn tail）。

snapshot 的 mapping 段用迭代器流式编码，不再先摊平成整表 `Vec<u64>`
（eng-76）。segment 段是可选段，只有该表存在活跃 segment mapping 时才写。

## 写入顺序（durability 链）

```text
写 data / overflow / segment 页
  → io.sync_data(tbl)        // fdatasync 本表 dirty 的 data / segment 文件
  → append_manifest(log)     // 或 switch_manifest(snapshot)
  → （switch 时）rename 后 fsync 父目录
```

对齐 C++ `WaitWrite → SyncData → FlushManifest`：manifest 引用的页一定已落盘。
compact / truncate / clean_expired 也先把改动写盘并 commit，再落 manifest；
manifest 失败时用 `abort_batch` 回滚 mapping / 分配器 / dirty 集合（core-25）。

## rotation

`manifest_limit`（默认 8 MiB）超过时写新 snapshot 并整体替换
（`switch_manifest`：临时文件 → sync → rename → fsync 目录）。
字典刚训练出时强制写 snapshot 以持久化 `dict_bytes`。

branch 模式同样走 log 追加；archive 是独立的 `manifest_<branch>_<term>_<tag>`。

## 恢复（reopen）

`fkv_recovery` 解析 manifest 字节流，得到 `ReplayState`：

- `root_id` / `ttl_root_id`
- `mapping_tbl` / `max_fp_id`
- `segment_mapping` / `max_segment_fp_id`
- `dict_bytes`
- `branch_metadata`

`Store::install_replay_state` 装回 mapper、root、字典（`load_dictionary` 成功后
放进 `compression_map`，即使当前未启用压缩也能读旧盘面），清空 index 缓存。
main 与 branch manifest 的 torn tail 在本地后端都会按最后一个完好 record 的
`valid_len` 截断（`reset_manifest` / `reset_manifest_file`），下一次写不再追加到
损坏区之后；`CloudStore` 的具名 manifest 仍走默认 no-op，远端对象截断需 cloud
专项处理。

`Store::reopen` 只负责读取并安装本地 manifest，`ResourceMissing` 原样上抛。
`fkv_shard::ReopenQueue` 只做**单次**延迟尝试，失败即连坐全部 waiter（对齐 C++
`CompleteReopenWaiters`）；`auto_reopen_retry_times` 是**每读请求**预算，挂在
请求上逐轮消耗，不是 driver 的多轮重试。外部 `Reopen` 无同表 driver 时立即回
`ResourceMissing`，不 park。其它错误立即返回。Cloud / Standby 的 reopen 先回源
或先追赶 master，再由 `install_external_snapshot` 安装。

## 崩溃一致性

`fkv_failpoint` 提供命名 `fail` / `kill` point，已接入 IO 关键路径
（写页、append manifest、switch manifest 的 sync/rename/sync-dir 等）。
`fkv_io_sim` 提供确定性模拟后端：`crash()` 丢弃未 fsync 的 volatile 写，
`restart()` 只保留 durable 视图，并可记录完整操作日志。

当前已有针对 torn tail、compact/truncate/clean-expired 提交失败、OOM 回滚、
term 栅栏的回归测试；**尚无**完整的 24h / crash fuzz / 随机 kill point 闭环。
`mapping` / `store` 类 fixture 也没有 C++ 字节实现，两者都不能算 V5/V2 已闭环。

## term

term 用于单 writer 仲裁（standby / cloud）。所有后端（Local / Mem / Cloud /
Sim）均实现 `TermIo`：`CURRENT_TERM_<branch>_<pgid>` 持久化，`upsert_term` 走
CAS，同 term 幂等，低 term 返回 `ExpiredTerm`。Cloud 侧单次 GET 同时取 value
与 etag，重试次数有上限；standby 安装 manifest 时按 term 对齐并清旧 term。

## 分支/归档生命周期

- `create_branch`：从父 manifest 派生，先 pre-compact，预留 file id / segment id
  区间，branch 写只动自己的区间。
- `delete_branch`：删 branch manifest，GC 后续回收仅 branch 引用的文件。
- `create_archive`：快照成带 tag 的 manifest，按 tag 倒序保留 N 份。
- `reopen_archive` / `delete_archive`：按 tag 提升归档快照 / 幂等删除该 tag
  的全部 term。
