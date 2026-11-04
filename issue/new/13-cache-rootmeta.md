# 13 P1：fkv_cache / fkv_root 未接入，Store 状态无上限

## 现象

- `fkv_cache::PageCache` 和 `fkv_root::RootMetaMgr` 有实现和单测
- 但 `grep` 生产代码没有消费者：
  - `crate/engine/fkv_store`
  - `crate/engine/fkv_runtime`
  - `crate/engine/fkv_shard`
- Store 使用自己的 `mapper_map/root_map/segment_mapper_map`，只增不淘汰
- `KvOptions.root_meta_cache_size`、`enable_data_page_cache` 等字段没有消费

## C++ 对照

- `RootMetaMgr`：per-shard RootMeta LRU，按 `root_meta_cache_size` 限制
- `PageManager`：index/data 页缓存、pin/unpin、LRU、swizzle
- `enable_data_page_cache` 控制 data page 是否缓存

## 影响

- 打开大量 table 后 Store 内存持续增长
- 没有 data page cache，重复读同一页仍走 IO
- 没有 pin/eviction 语义，后续并发读也无法安全共享页
- 配置项看似存在但实际无效

## 建议

1. 用 `RootMetaMgr` 替换 Store 的裸 HashMap，或给现有 map 加 LRU/容量上限。
2. 接入 `PageCache` 作为 index/data page cache：
   - index 页默认常驻（见 02）
   - data 页受 `enable_data_page_cache` 控制
3. 实现 pin/refcount，避免 eviction 正在使用的页。
4. 将 cache 命中/evict 计入 metrics。

## 验收

- 打开 N 个 table 后内存不超过配置上限
- 重复 get 同页在 cache 命中后无 IO
- eviction 不影响正在执行的请求
