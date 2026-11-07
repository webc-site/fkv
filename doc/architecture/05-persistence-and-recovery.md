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
损坏区之后。`CloudStore` 没有另起一条路径：`reset_manifest`（活跃 manifest）与
`reset_manifest_file`（具名 manifest，branch / archive）都直接转发给本地后端，
因此两类 manifest 在 replay 后同样只截断本地 cache，reopen 热路径不回写远端。
若远端对象自身带撕裂 / 垃圾尾，截断后保持原样直到下一次 append——append 以
截断后的本地文件为基线整文件 PUT，把远端覆盖成干净前缀
（`fkv_cloud/src/manifest.rs`）。

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
term 栅栏的回归测试。V5 的确定性闭环已落地（gap-04）：

- **op 边界穷举**：`fkv_io_sim/tests/op_log_replay.rs` 对 6 条固定 op log
  （纯写、写删、TTL、truncate、archive+branch、reopen 交错）在每个逻辑 op
  索引 `kill_point = 0..=len` 处 `crash() + restart()`，重开后与同前缀干净
  运行的规范化 dump 逐字节比较；branch 场景还逐边界校验 `open_branch` 内容。
- **intra-op 命名 kill point**：`fkv_store/tests/crash_failpoint_invariants.rs`
  在真实 compio 后端用子进程对 7 个 kill 名字 × 命中序号（`name@N!kill`，
  默认 `N <= 6`，`FKV_CRASH_HIT_MAX` 可调）逐个 `abort()`；父进程 reopen 后
  校验可见状态等于某个干净前缀，并确认 attach 清掉了残留 `*.tmp`。另有
  `SyncData` / `WriteSegments` / `WriteFile:SyncFail` 三个 fail-only 点作为
  反向哨兵，断言它们不会被 `!kill` 误触发。
- **失败可复现**：两条用例的失败信息都包含 `seed`、`kill_point`（注入名 +
  命中序号）、`op_index`；intra-op 用例还打印可直接粘贴的子进程复现命令
  （`FKV_CRASH_SEED` / `FKV_CRASH_ROOT` / `FKV_CRASH_PREFIX` /
  `FKV_FAILPOINT`）与整条用例的 nextest 复现命令。默认 seed 固定，
  `FKV_CRASH_SEED` 可覆盖并实际改变写入 value 的字节。
- **反向验证**：`crash_clean_prefix_checker_rejects_bad_state` 把测试内构造的
  半值、值尾多字节、缺键、多键、顺序颠倒等坏 KV 喂给生产路径真正使用的
  `check_clean_prefix`，断言其返回 `Err`；同时用合法前缀做正向对照，证明
  检查器既不是恒真也不是恒错，不依赖任何伪造的生产缺陷。

仍未闭环的边界：

- 没有 24h 持续 fuzz，也没有指令级随机 kill point：覆盖集合仍是**命名**
  kill point × 命中序号，intra-op 默认只枚举到第 6 次命中。
- sim 无目录项持久化，`SyncDir` / `AtomicWriteFile:SyncDir` /
  `AppendManifest:SyncFail` 在 sim 侧结构性不可达；`CloudStore` 的
  `CloudManifest:Put` 未纳入本闭环。
- 进程 `abort()` 不模拟内核页缓存丢失或真实掉电，撕裂与目录项回退仍由后端
  实现语义约束。
- 引擎级 V3（Rust/C++ 同 op log 差分）未执行，`mapping` / `store` 类 fixture
  也没有 C++ 字节实现，因此 V5 闭环不等于 V5/V2 与 C++ 完全等价。

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
