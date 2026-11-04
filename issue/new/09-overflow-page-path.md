# 09 P0：Overflow page 只有 codec，读写路径未实现

## 现象

- `crate/format/fkv_page/src/overflow.rs` 已实现 overflow page 编解码
- 但 read / scan / write 路径遇到 `PageType::Overflow` 直接报错：

```rust
// crate/engine/fkv_task_read/src/lib.rs:74
PageType::Overflow | PageType::Deleted => return Err(KvError::Corrupted),
```

`fkv_task_write` 也没有 overflow value 的拆分/链式写入；大 value 目前完全走 segment。

## C++ 对照

eloqstore 的 value 形态有三类：

1. inline data page
2. overflow page chain（大于单页、但低于 very-large threshold）
3. segment pointer（very large value）

leaf entry 的 `ValLenBit::Overflow` 指向 overflow page 数组，overflow page 末尾保存继续指向下一页的 pointer。

## 影响

- 读不了 C++ 产生的 overflow 盘面
- 在未启用 large value 的 Store 上，value 超过单页会失败
- 字节级 L1 兼容无法覆盖这类 key
- 与 segment 路径的边界不完整

## 建议

1. 在 `fkv_task_write` 增加 overflow 分配/链式写入：
   - value 超过单页容量且未达到 large threshold 时，按 C++ 的 `overflow_pointers` 拆分
   - 将 encoded pointer 数组放入 leaf entry，设置 overflow bit
2. 在 `fkv_task_read` / `scan` 增加 overflow 链解析，按顺序拼回 value。
3. 与 large value 的阈值保持一致，避免两种路径重叠。
4. 用 C++ fixture 验证 pointer 数组和 overflow page 尾部布局。

## 验收

- 略大于 page_size 的 value 可正常 put/get/scan/reopen
- C++ 生成的 overflow fixture 可被 Rust 读出
- Rust 生成的 overflow fixture 可被 C++ 读出
