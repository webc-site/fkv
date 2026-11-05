# 06 已知差异与缺口

## 存储模式

- 只实现 append 分配模式。`data_append_mode=false` 的 pooled 原地复用分配器
  不迁移；`validate` 会对非默认值告警。
- 单进程只托管一个 attach 身份。`partition_filter` / 多 data store shard
  fanout 不实现；请求路由固定按 `partition_id % shard_num`。
- data page cache / 用户态 page buffer pool 不实现：data 侧依赖内核 page cache
  或 direct IO，index 侧由 `fkv_gc` / `IndexCache` 的 Clock + 代际策略覆盖。

## 格式

- branch + large value：创建 branch 时写 segment 段，保留 large value 继承与
  跨分支 segment file_id 区间隔离；该组合不承诺与其它实现逐字节一致。
- data page 四分页重分配的切分点按 entry / 边界选择字节中点；生成页满足格式，
  同一输入不保证逐字节相同。
- manifest record framing 使用 OS 页大小（Linux 4 KiB、Apple Silicon 16 KiB）；
  跨页大小主机不保证 manifest 互读。

## 恢复

- V5 崩溃恢复已有确定性闭环（详见 `05-persistence-and-recovery.md`）：
  `fkv_io_sim` 穷举每条 op log 的逻辑 op 边界，真实 compio 后端穷举命名
  kill point × 命中序号；失败信息含 `seed` / `kill_point` / `op_index` 与
  复现命令，前缀检查器另有反向验证。未覆盖 24h 持续 fuzz、指令级随机 kill
  point、sim 目录项语义与 `CloudManifest:Put`。
- 撕裂尾覆盖与 GetMapper 分配器重建按当前实现语义处理，不承诺与其它实现
  完全等价。
- StandbyReplica 懒打开会先 `sync_standby_partition` 追一次 master；这是有意
  加强，不改变盘面格式。

## 配置

以下旋钮解析但不生效，`KvOption::validate` 对非默认值告警：

`enable_data_page_cache`、`init_page_count`、`coroutine_stack_size`、
`max_archive_tasks`、`reserve_space_ratio`、`direct_io_buffer_pool_size`、
`chunk_size`、`gc_global_mem_size_per_shard`、`pinned_tail_scratch_slots`、
`cloud_request_threads`、`write_buffer_size`、`write_buffer_ratio`、
`non_page_io_batch_size`、`segment_compact_yield_every`。

## 性能

- `fkv_bench --key-interval` 是均匀网格；同参数下的键分布与随机步进不同。
  跨实现比较只在 `key_interval = 1` 时严格可比。
- 冷数据点读恰好 1 次磁盘 IO；批量写不阻塞读。
