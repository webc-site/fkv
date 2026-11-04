# 01 overflow 页编解码齐全但热路径从不使用

**严重度**：P0（盘面不兼容）

## 现象

`fkv_page/src/overflow.rs` 完整实现了 overflow 页的 builder / parser（header 11B + value + pointers + num_pointers）。

但引擎从不产生也不消费：

- `merge_data_pages_with_compression` 调用 `builder.add(...)` 时 `overflow` 参数**恒为 false**（`crate/core/fkv_btree/src/lib.rs:196、213`）。
- 全仓库除 `fkv_page` 自身测试外，没有任何代码引用 `OverflowPage`。

## 后果

C++ 对「放不进一个 data page、但没到大 value 阈值」的值走 overflow 页指针链（`GetOverflowValue` 按 `max_overflow_pointers=128` 分批走链）。

Rust 侧 `DataPageBuilder::add` 放不下时直接返回 `false` → 拆新页，**永远不会写 overflow 页**。

于是：

- Rust 写出的盘里没有 overflow 页，C++ 读没问题（没引用）。
- 但 C++ 写的盘里若存在 overflow 页，Rust 读路径遇到 `PageType::Overflow` 会直接 `Err(KvError::Corrupted)`（`fkv_task_read/src/lib.rs:74`、`fkv_task_write/src/lib.rs:149`）。
- 大 value 语义不对齐：值大小落在 `(page 可容纳, large_value_threshold)` 区间时行为分歧。

## C++ 对照

- `include/storage/data_page.h:393-441` overflow 页布局，`header_size=11`。
- `src/tasks/task.cpp:173-220` `LoadOverflowPage` / `GetOverflowValue`。
- `include/kv_options.h:393` `overflow_pointers` 默认 16（max 128）。

## 建议

1. 写路径：值放不下单页时写 overflow 页（多个），data page 里存 pointer 链，flag bit0 置位，对齐 C++ `WriteOverflowValue`。
2. 读路径：见 `PageType::Overflow` 时按指针链读回，不要报 Corrupted。
3. 补 V4 交叉读：C++ 写含 overflow 的 store → Rust 全量 scan。

## 验收

- C++ 生成的 overflow 样例 store，Rust 能读出与 C++ 一致的 key/value。
- Rust 写的 overflow 值，C++ 能读。
