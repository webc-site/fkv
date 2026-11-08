# compio 0.19.2 已知上游缺陷与暴露面

状态：2026-11-06。fkv 锁定 `compio = "0.19.2"`（`Cargo.toml:21`；锁文件
`compio-driver 0.12.5` / `compio-executor 0.1.4` / `compio-buf 0.8.3` /
`compio-runtime 0.12.6`）。上游 `v0.20.0-beta.1` 已含其中两条修复，但 0.20
仍是 beta，本轮不升级；本文件登记缺陷、已核实暴露面与不依赖升级的缓解。

## 上游修复与版本关系

| # | 上游 commit | 行为 | 进入版本 |
|---|---|---|---|
| 1 | `735e473c` fix(executor): prevent Waker from blocking | 跨线程唤醒满队时生产者不再自旋 | `v0.20.0-beta.1` |
| 2 | `420f3c00` fix(driver/iour): wake after draining a full SQ | SQ 满后排空 CQE 会重新安排唤醒 | `v0.20.0-beta.1` |
| 3 | `4b539841` fix(executor): use the tick budget for newly woken tasks | 新唤醒任务纳入 tick 预算 | beta 之后（无 tag） |
| 4 | `0143e5fe` fix(driver, iour): Bump io-uring dependency to 0.7.15 | compio-driver 最低 io-uring 提到 0.7.15 | beta 之后（无 tag） |
| 5 | `36d1fd0b` fix(executor): drop tasks outside the queue borrow in clear | `executor.clear()` 在队列借用外析构任务 | beta 之后（无 tag） |

`git tag --contains` 核实：1 / 2 只含 `v0.20.0-beta.1`，均不含 `v0.19.2`；
3 / 4 / 5 不含任何 tag（beta 之后）。

## 缺陷与暴露面

### 1. 跨线程唤醒队列满 → `Remote::schedule` 自旋

0.19.2 的 `compio-executor 0.1.4` 用容量 64 的 `ArrayQueue` 承载跨线程唤醒
（`src/lib.rs:117,127,166`）；满队时 `src/task/remote.rs:63-77` 在
`yield_now` 循环里自旋等消费者腾位。上游改为无界 `SegQueue`。

**已核实暴露面（test-05）**：`fkv_shard/src/lifecycle.rs` 的 `Shard::shutdown` 自述
shutdown 时 shard 线程 `fail_pending` 的批量 oneshot 回执打满调用方
compio runtime 的 64 槽唤醒队列，双方互等。现用「join 移到短命线程」
（同函数内 `fkv-shard-join` 线程）绕过，根因仍在：任何「A 线程批量回执 + B 线程暂时不能泵
任务队列」的组合都会退化。

**当前缓解**：

- join 线程绕过保留（升级后仍保留，防御性无害）。
- `finish_read_li` 单次屏障累计回执数 > 64 时发
  `warn!(target: "fkv_shard", "read barrier reply burst ...")`，让生产可见；
  只告警，不改变回执节奏。

### 2. SQ 满后不自我唤醒 → `poll` 永久阻塞

0.19.2 的 io_uring driver 在 SQ 满、排空 CQE 腾出 SQE 空间后不安排唤醒；
若无外部事件，线程阻塞在 `poll` 不返回。上游 `420f3c00` 带回归测试
`poll_full_submission_queue`（容量 8 压 24 个写后 `poll(None)` 挂死）。

**已核实暴露面**：`build_runtime` 把用户可配 `io_queue_size` 直接设为 ring
容量（`lifecycle.rs` 的 `build_runtime`），旧 `validate` 只拒 0。读窗口上限
`MAX_READ_IN_FLIGHT = 128`（`fkv_shard/src/lib.rs:43`），单条链读按
`overflow_pointers`（默认 16）逐页压 SQE；两次 enter 之间最坏可压
128 × 16 = 2048 个 SQE，再叠加写组突发。默认 4096 能覆盖，调小后不能。

**当前缓解（io-10 配置护栏）**：

- `fkv_config::validate` 拒绝 `io_queue_size < MIN_IO_QUEUE_SIZE`（256）。
  依据：128 条在飞读即使每条 1 个 SQE 也占 128 槽，256 只留一倍余量。
- `io_queue_size < IO_QUEUE_SIZE_WARN_BELOW`（1024）告警。依据：128 × 8 页
  = 1024；默认 `overflow_pointers=16` 的最坏突发是 2048，只有默认 4096
  完全覆盖。
- 默认值不变；单分区写严格串行，跨分区并发由 `max_write_concurrency` 限制
  （Local 默认 `0` = 不限，因此写侧不构成额外上界），IO 层仍有
  `max_inflight_write` 闸门。

### 3. 新唤醒任务不受 tick 预算约束

`4b539841` 修的是 executor 对新唤醒任务的公平性。与 io-08 的调度公平面
相关：极端情况下持续唤醒的任务可能挤占同一 shard 上的低优先维护与请求
边界，表现为尾延迟，不会数据损坏。当前 `max_processing_time_us` /
`yield_budget_us` 只约束单任务内长循环和请求边界，覆盖不到 executor 层。
升级后重点回归公平性用例。

### 4. io-uring crate 层修复

`0143e5fe` 只把 `compio-driver` 的最低 `io-uring` 依赖从 0.7.13 提到 0.7.15。
fkv 直接依赖并锁定 `io-uring = "0.7.15"`（workspace 依赖；
`fkv_io_compio/src/fixed.rs` 使用），`Cargo.lock` 统一解析为 0.7.15，
**该 crate 层修复实际已在树内**。剩余风险只是 compio-driver 0.12.5 发布时的
最低要求仍是 0.7.13，该组合未由 compio 上游固定；当前无已知暴露面。

### 5. `executor.clear()` 队列借用内析构任务

`36d1fd0b` 把任务析构移到队列借用之外，避免 clear 期间重入。fkv 的
`Runtime::drop` 会走到 compio runtime 的 `executor.clear()`；drop 发生在
shard 线程 `block_on` 返回后，不在任务 poll 栈内，理论重入窗口窄。未观测到
问题，升级后随 `shutdown_race` / lifecycle 用例一起回归。

## 升级到 compio 0.20 的回归清单

升级门槛：0.20 正式版（或团队明确接受 beta）。升级分支上至少跑：

- `cargo nextest run --workspace --all-features`（全量）
- `fkv_shard/tests/shutdown_race.rs`（缺陷 1、5）
- 高并发读写：`read_concurrency.rs` / `read_spawn.rs` /
  `write_concurrency.rs` / `write_gate.rs` / `queue_backpressure.rs`
- failpoint：`fkv_io_compio/tests/failpoint.rs`、`fixed.rs`、
  `fixed_write_cancel.rs`
- fixed 写：`fkv_io_compio/tests/fixed.rs` / `write_pool.rs`
- managed 读：`fkv_io_compio/tests/managed_read.rs`
- `fkv_shard/tests/lifecycle.rs`、`partition_barrier.rs`
- 0.20 的 `sync_queue_size` 已废弃（字段仍在，设置被忽略）：检查配置 /
  代码引用（fkv 当前未设置）。

升级后保留：

- `Shard::shutdown` 的 join 线程（与 executor 实现无关的防御）。
- `io_queue_size` 硬下限与告警（对 0.20 仍是容量护栏，阈值按新
  executor / driver 行为复核）。
- `read barrier reply burst` 告警（0.20 无自旋后用于观测单次屏障的累计
  回执数）。

## 相关文件

- `crate/foundation/fkv_config/src/lib.rs`：`MIN_IO_QUEUE_SIZE` /
  `IO_QUEUE_SIZE_WARN_BELOW` / `validate`。
- `crate/foundation/fkv_config/tests/config.rs::io_queue_size_guard`：护栏单测。
- `crate/engine/fkv_shard/src/lifecycle.rs`：ring 构建、test-05 join 绕过。
- `crate/engine/fkv_shard/src/read/finish.rs`：`finish_read_li` 回执批告警。
- `crate/engine/fkv_shard/tests/shutdown_race.rs`：shutdown 回归入口。
