# 07 P1：Scan 逐页串行读取，没有 prefetch / batch

## 现象

`scan_page_with` 的主循环每次只加载下一页：

```rust
// crate/engine/fkv_task_scan/src/lib.rs:134
loop {
  ...
  page.fill(0);
  io.read_page(tbl, file_page_id, &mut page).await?;
  iter = DataPageIter::new(&page)?;
  has = iter.advance()?;
}
```

没有预读下 N 页，也没有批量提交多个 page IO。

## C++ 对照

- `ScanRequest` 有 `PrefetchPageNum`，受 `max_read_pages_batch` 限制
- `ScanTask`/`ScanIterator` 维护 `prefetched_pages_`，一次发起多页读
- 读下一页时通常已经在内存

参见 `docs/architecture/03-request-api.md` 的 Scan 说明。

## 影响

- 每页一次 open/read/await，scan 吞吐受 RTT 限制
- 无法利用 io_uring 的并发深度
- 跨页 scan 的尾延迟明显高于 C++

## 建议

1. 按 `next_page_id` 链预取 N 页（默认 4/8），用 `FuturesUnordered` 或 compio 批量提交。
2. 预取上限受配置 `max_read_pages_batch` 控制。
3. 与 page buffer/PageManager 结合，避免预取时反复分配。
4. 分页 `page_entries` / `page_size` 限制仍在逻辑层裁剪，预取只影响 IO。

## 验收

- scan 的 page IO 呈批量提交而非串行
- `scan_kv_s` 接近 C++ `simple_bench`
- 预取失败时可回退到顺序读
