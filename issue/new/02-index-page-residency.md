# 02 P0：索引页没有常驻内存，点读每层都产生 IO

## 现象

`fkv_task_read::read_entry` 从 root 开始，每下降一层都调用 `io.read_page`：

```rust
// crate/engine/fkv_task_read/src/lib.rs:51
for _ in 0..MAX_TREE_DEPTH {
  let file_page_id = mapper.file_page_id(page_id)?;
  io.read_page(tbl, file_page_id, &mut page).await?;
  ...
}
```

Store 没有索引页 cache；`Store::mapper_map/root_map` 只保存 mapping/root id，不保存解码后的 index page。

## C++ 对照

eloqstore 的设计是：

- **所有 index pages 在 partition 打开期间常驻内存**
- `MemCachedPage` 同时缓存 index page 和可选的 data page
- `MappingSnapshot` 可以把 child 引用 swizzle 成 `MemCachedPage*`
- 点读热路径只做一次 data page IO

参见 `docs/architecture/05-btree-and-page-formats.md` 的 “In-memory index pages are not the disk bytes” 和 “Swizzling” 两节。

## 影响

- 树高 3 时，一次 get 至少 3 次 index IO + 1 次 data IO；C++ 是 1 次 data IO
- 每个 index IO 还要 open + 分配 + copy（见 05）
- scan 每次跨页也重复读 index
- 固定 buffer 只用于 segment，不覆盖 page IO

这是当前与 C++ 最大的单点读性能差距。

## 建议

1. 为每个 table 增加 index page cache：
   - key: `(TableIdent, PageId)`
   - value: 解码后的 `Vec<(key, child_page_id)>` 或原始 page bytes + 解析缓存
   - reopen 时按需加载，写入 COW 时更新
2. 中期实现 `MappingSnapshot` 的 swizzle 或等价缓存，避免每次查找再走 page id。
3. 给 index cache 加命中率指标，并纳入 bench。

## 验收

- 深度 >= 3 的 get，index IO 次数在 warm cache 后为 0
- `fkv_bench` 的 `get_kv_s` 接近 C++ `simple_bench`
- reopen 后首次读能正确填充 cache
