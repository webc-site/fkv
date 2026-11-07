# 04 B+tree 与页格式

## 页

4 KiB 默认（`page_size` 可配）。每页头：`checksum(8B) | page_type(1B)`。

| 类型 | 布局 |
|---|---|
| Data | `header(19B) | entries | restart array | restart_num(2B)`；entry 键前缀压缩 + restart 点存全键；value 长度低位编码 `overflow / expire / compression(2b)`，`0b11` 表示 large value（segment 指针） |
| Index（Leaf/NonLeaf） | 同前缀压缩 + restart；entry 为 `key -> child PageId`，另有 `leftmost_ptr` |
| Overflow | `header(11B) | value | pointers(N*4B) | num_pointers(1B)` |
| Deleted | 占位 |

`fkv_page::restart` 提供页内查找的共通逻辑：对 restart 数组二分定位，再在该区间
内单遍推进，页内比较 O(log(restarts) + interval)。`IndexNode::seek`、data `seek`、
`seek_floor` 都走它；`seek_floor` 单遍完成（窥探下一 key，超出即停）。

## 内存中的索引页

index page 由 `fkv_btree::IndexCache`（`PageId -> IndexNode { leftmost, entries }`）
按需回填：`fkv_task_read` / `fkv_task_scan` 的 index 层下降命中缓存即跳过 IO，
warm 后一次点读只剩 data page IO。`IndexNode` 支持原始页懒 seek，打开页时不
全量物化。

`IndexCache` 有字节预算与 Clock（二次机会）淘汰（`Store::with_index_cache_bytes`，
`KvOption::index_cache_bytes`）；命中要求条目代际与读者 `MappingSnapshot` 的
映射代际一致，旧快照不会命中新代节点。命中 / 未命中 / 逐出 / 占用经
`Store::index_cache_stat` 汇入 shard 的 `cache_*` 指标。

聚合上限由 `Store::with_index_cache_total_bytes`（`KvOption::index_cache_total_bytes`）
提供：多表共享一份预算，按挂载的缓存实例数均分，单表实际上限取
`min(每表预算, 聚合预算 / 表数)`；两键独立，`0` 均表示不限制。

fkv 不缓存 data 页（直读），只对 index 做聚合记账，且按表均分额度，
热点表不能占用冷表份额。

## COW 写路径

`fkv_task_write::write_batch`：

1. batch 按 key 去重排序，单次有序下降定位受影响 data page（不再双重下降）。
2. `merge_data_pages_with_options` 合并旧页与 batch：按 timestamp 过滤、
   删除项移除、必要时拆页；页内已有 entry 保留原字节与 compression flag
   （绝不二次压缩）。
3. 页满放不下的**非 large、非指针**值 → 生成 overflow 占位 entry +
   `OverflowSlot`；由 `write_overflow_values` 落多级页链并回填首批指针。
4. 只重写 root→leaf 路径上的 index page；子页填充率低时按 C++ 的四分页
   局部重分配处理。
5. 写页 → `io.sync_data(tbl)` → 写 manifest（见 `05-persistence-and-recovery.md`）。

同一分区的连续 Put 在 shard 层可合并为一次 `Store::put`（group commit）；
`Store` 内部仍按上述 COW 路径执行，一次 manifest append + fsync。

## 值形态

| 形态 | 判定 | 存储 |
|---|---|---|
| inline | 常规 | data page 内 |
| 压缩 | `>=100B`；字典可用时 Dictionary，`>10MiB` Standalone | 见下 |
| overflow | 超过单页容量且非 large | 多级 overflow 页链，leaf 存首批指针 |
| large value | `len >= large_value_threshold` | segment 文件 + metadata trailer，leaf 存编码指针 |

压缩用 `mzstd`（纯 Rust，与 zstd 1.5.7 字节一致），格式
`varint32(original_len) | zstd_frame`，对齐 C++ `CompressRaw`。字典按表维护
（`DictCompression`），写路径自动采样训练、随 manifest `dict_bytes` 持久化，
reopen 恢复。线程内复用 `Compressor` / `Decompressor`。

## TTL 树

除主树外每分区一棵 TTL 树，key = `expire_ts(8B BE) | user_key`。
写时 `expire_ts <= now` 的 upsert 不落主树；写批重写 data 页时，基页上
`expire_ts <= now` 的旧条目也在该批内直接丢弃（释放 overflow 链 / large
value 段并删除对应 TTL 索引项，对齐 C++ `BatchWriteTask::add_to_page`）。
写后若已知最早到期则触发清理。
`clean_expired` 只扫 `[空, ttl_key(now+1))` 区间，并 peek 一条未过期项维护
`next_expire_ts`；清理批按单轮上界分批，不做 now 过滤（只删除扫描结果，
未扫描到的过期条目由后续轮次处理）。

## 分支与归档

- branch 有自己的 manifest `manifest_<branch>_<term>`，从父快照派生
  mapping 与 file id 区间；branch 写走 log 追加（不再每次整份 snapshot）。
- archive 生成 `manifest_<branch>_<term>_<tag>`，按 tag 倒序保留 N 份。
- GC 汇总 active + 所有 branch/archive 的 retained file page 集合后再删文件；
  按 `(branch, term)` 变体精确删除。
- branch + large value：创建 branch 时写 segment 段，保留 large value 继承。

## 已知缺口

当前无未收敛的 btree / 格式缺口。root 状态字节预算 LRU、跨 `(branch, term)`
的 GC 变体粒度、segment 批量 fixed 读均已落地。split / index 重平衡、
mapping 分块、COW 读快照已落地；data 页缓存不做。
