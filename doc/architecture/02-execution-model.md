# 02 执行模型

## 线程与运行时

```
Runtime
├── Shard 0 .. Shard N-1        每个 = 1 OS 线程 + 1 compio Runtime
├── ArchiveCrond                （可选）后台归档线程
└── PrewarmCrond                （可选）Cloud 预热线程
```

- `Runtime::start(RuntimeConfig)` / `start_with_option(RuntimeConfig, &KvOption)`
  起 N 个 shard 线程；每个线程内创建 compio `Runtime` 与具体 IO 后端。
- 请求按 `TableIdent::shard_index(n) = partition_id % n` 路由到 shard
  （对齐 C++ `TableIdent::ShardIndex`）。
- 跨线程只经 `crossfire` 有界 MPSC + `oneshot` 回执；shard 内无锁。
- `shard_id` 是 attach/detach/term 的作用域；`Runtime::attach_shard` /
  `detach_shard` 按 shard 广播，不是按表。

## 请求通道与优先级

每个 shard 有两条有界通道：

| 通道 | 请求 | 调度 |
|---|---|---|
| 高优先 `tx_hi` | 前台 Get / Put / Scan / Floor / Attach / Detach / Shutdown / Reopen；以及开销小的控制面观察 ListArchiveTags / ListStandbyPartition | 默认通道；连续服务超过 `max_processing_time_us`（默认 400us）时先插一条低优先 |
| 低优先 `tx_lo` | FileGc / LocalGc / Compact / CreateArchive / DeleteArchive / DeleteBranch / CleanExpired / Prewarm / ListObject | 只在没有前台请求、或高优先超时间片时各跑一条 |

低优先深度由 `ShardSender` 入队时记账（`low_pq_depth` gauge），消费侧递减；
投递失败或被取消时回滚。高/低优先与时间片的测试见
`crate/engine/fkv_shard/tests/priority.rs`。

## 请求生命周期

```text
Runtime::put(tbl, batch)
  └─ ShardSender::request(Request::Put)
       ├─ tx_hi.send(Envelope { req, reply, cancel, deadline })
       └─ await reply / timeout / drop
Shard 线程:
  loop:
    1. 先推进已到期的 reopen 重试；
    2. 有排空条件时 drain 在飞读（写 / 控制请求需要独占 &mut Store）；
    3. 从高优先通道取请求；连续只读请求聚成读批（≤ MAX_READ_BATCH = 32）；
    4. 只读请求 spawn 成独立 compio task；非只读请求进 WriteGate 或直接处理；
    5. 低优先通道按时间片插入。
```

- **读并发**：Get / Scan / Floor 先串行做准入与 `ensure_table_ready`，再 `spawn`
  成独立任务，读任务只持 `Rc<Store>`；读任务集合是 `FuturesUnordered`，窗口上限
  32。不同表、不同读之间可真正交错 IO；`fkv_shard/tests/read_spawn.rs` 覆盖 96
  并发读与「慢 scan 不阻塞其他表 get」。`GetLarge` 借用调用方的 fixed buffer，
  仍在独占路径串行执行（不计入读窗口）。
- **写并发**：写请求进 per-partition FIFO `WriteGate`，同分区严格按入队顺序执行；
  同分区、同 term 的连续 Put 合并为一次 `Store::put`（group commit，一次 manifest
  fsync）。不同分区按 `max_write_concurrency` 真正并发 `spawn`，全局额度由
  write gate 计数。
- **读写隔离**：任何写 / 控制 / 维护请求开始前，shard 先 `finish_read_li` 等在飞读；
  这既保证 `Rc::get_mut` 成功，也保证「先到的读先完成」与 attach/detach 线性化。
- **取消与截止**：`RequestHandle` drop 置 cancel；shard 在请求开始前检查，
  已开始的请求不中途取消（与 C++ 一致）；超时 / 截止在出队后执行前判。
- **OOM 重试**：`OutOfMem` 时 `Store` 已回滚本批 mapping / 分配器，请求整体重跑，
  次数由 `auto_oom_retry_times` 控制。
- **请求内让出**：scan / overflow / GC 等长循环用 `YieldBudget`（默认 20us）
  主动让出，避免单请求吃满 shard 线程；见 `ShardConfig::yield_budget_us`。

## 归档与预热

- **ArchiveCrond**：`archive_interval_secs > 0` 时起一条后台线程。定时器与停止
  信号共用 `crossfire::bounded_blocking` 通道（`recv_timeout` = 定时；发送端
  drop = 停止）。一轮归档并发向所有已 attach 的 shard 投递 `CreateArchive`，
  在顶层线程 `block_on`（不在任务 poll 栈内）。
- **PrewarmCrond**：Cloud 模式下 `prewarm_cloud_cache=true` 时起后台线程，
  按 attach 身份列举远端分区并下载到本地 cache；每表可指定并发。
  `Runtime::prewarm_report()` 取最近一轮回执。
- 表注册表在 shard 线程内用普通 `HashMap` / `BTreeMap`（thread-per-core，
  无跨线程共享）；跨线程只经通道。`papaya` 已在 eng-59 移除，热路径无全局锁。

## 生命周期与 term

- `AttachShard { branch, term }` / `DetachShard`：attach 前除 attach/detach/shutdown
  外的请求返回 `NotRunning`。
- 写入可带 `term`；shard 校验 `state.term`，不一致返回 `ExpiredTerm` 并把整个
  shard 标记为 `Expired`（group commit 快路径与单请求路径一致）。
- `Expired` 是**整 shard** 的粘滞快速失败态（`fkv_shard` 请求准入处）：除
  attach/detach/shutdown 外，读（Get / GetLarge / Scan / Floor）、列举
  （ListArchiveTags / ListObject / ListStandbyPartition）与写一律返回
  `ExpiredTerm`，直到同一 branch 以不低于当前 term 重新 attach 或 detach。
  触发源不止写入：`Drop` 读 term 文件发现更高 term、attach 用旧 term 也是同一
  转换。
- shard 线程内独占 `lifecycle`，对外只经 `fkv_shard::ShardLifecycle` 暴露语义化
  判定（`is_attached` 等）；`LifecycleState` 是 crate 私有的镜像编码，不对外。
  运行时在所有 shard 发送的
  统一收尾（`ShardRoute::finish`）里读该镜像：请求出错后若 shard 已不处于
  `Attached`（本地 `Timeout` 时请求可能已执行并置 `Expired`），清掉本层 attach
  视图（attach 身份 / 归档目标 / 预热路由）；清理按 per-shard attach 代际
  校验，迟到错误不会清掉更晚成功的 attach 新视图。
- `DetachShard` 会驱逐该 shard 已加载的表状态（`Store::evict_table`），
  并清空写队列与挂起的 reopen。

### attach / detach 的控制面契约

- **per-shard 串行**：`Runtime::attach_shard` 与 `Runtime::detach_shard` 共用
  `control_li[shard_id]` 异步锁。锁内固定顺序为「attach 代际 +1 → 发
  `AttachShard` / `DetachShard` → 落地或清除本层视图」，因此并发的
  attach / detach 不会交错，`!recorded`（视图被更晚操作接管）在生产路径不可达，
  只剩防御作用。
- **锁序**：`control_li` → `attach_epoch` 门 → 视图锁（`attach_ident_li` /
  `archive_state` / `prewarm_state`）。数据面请求与
  `ListStandbyPartition` 不取 `control_li`，只读代际快照做统一收尾
  （`ShardRoute::finish`）；反向获取（先视图锁再 `control_li`）会死锁。
- **错误面**：
  - `InvalidArgs`：`shard_id` 越界或 `branch` 无法归一化。
  - `ExpiredTerm`：`term` 低于 shard 已见 term，shard 粘滞为 `Expired`。必须
    以不低于当前 term 重放 attach（或先 detach）才能恢复。
  - `Busy`：可重试。来源是入队背压超时，或视图被更晚的控制操作接管（防御
    分支）。两种终态（shard 仍 `Attached` / 已 `Detached`）都回 `Busy`，
    绝不回 `Ok(())`：后者会让调用方以为 attach 成功，而后续请求实际全部
    `NotRunning`。调用方（CLI / SDK 宿主）应在 `Busy` 时重放 attach，重放期间
    不得假定 shard 已 attach。
  - `NotRunning` / `Timeout` / `Corrupted`：来自 shard 发送端与回执校验，按
    数据面语义处理。
- **成功语义**：只有 `Ok(())` 表示 shard 已 `Attached` **且**本层视图
  （attach 身份 / 归档目标 / 预热路由）已落地；其它返回都不得视作 attach
  成功。
- **观测 deadline**：`ListStandbyPartition` 不改变 attach 状态，但 shard 侧同步
  执行 `ssh` / `ls`（`fkv_standby/src/rsync.rs`），master 卡死会阻塞 shard 线程。
  该请求走带 deadline 的观测路由：deadline 取 `with_request_timeout` 的配置，
  未配置时用 `DEFAULT_OBSERVE_TIMEOUT`（30s），超时回 `Timeout`，不会永久挂起
  调用方。它仍不经过按 `TableIdent` 路由的数据面入口，保持「控制面观察请求
  不被拒绝」的语义。

## 指标

`ShardMetric` 记录每请求 op / 延迟直方图 / page 与 manifest 字节 / fsync 数，
以及调度与队列 gauge：

- `write_queue_depth`：排队等待启动的写请求数（跨分区合计）；
- `write_running`：当前在跑写任务数；
- `read_in_flight` / `read_in_flight_max`：在飞读任务数与进程内峰值；
- `read_batch_size`：每轮合并提交的读批大小直方图；
- `io_in_flight` / `io_in_flight_max`：shard 在飞 IO 数与峰值；
- `low_pq_depth`：低优先通道排队深度；
- `yield_total` / `round_over_budget_total`：让出与超时间片次数。

`Runtime::metrics()` 取全部 shard 的快照。
