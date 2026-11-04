# 08 数据页从不 fsync，manifest 先于数据落盘

**严重度**：P0（崩溃一致性）

## 现象

- `Store` 全程不调用 `io.sync_data`（grep 确认，除 trait/impl 外零调用）。
- 更糟：`CompioStore::sync_data(tbl)` 实际 sync 的是 **manifest 文件**，不是数据 / segment 文件（`crate/io/fkv_io_compio/src/lib.rs:394-403`）。
- manifest append / switch 各自 `sync_data`（lib.rs:420、430），但数据页写完从不等 fsync。

## C++ 对照

`WriteTask::UpdateMeta` 顺序严格：`WaitWrite()`（排空页 IO）→ `IoMgr()->SyncData(tbl)`（fdatasync 所有 `dirty_` 的数据 / segment fd）→ `FlushManifest()`（`src/tasks/write_task.cpp:854-896`；`src/async_io_manager.cpp:1310-1340`）。

即 **数据先落盘，manifest 后落盘**，manifest 引用的页一定已在盘上。

## 后果

Rust 侧 manifest 可能 fsync 完成而它引用的新数据页还在 OS page cache。断电后：replay 出 manifest 里的 mapping，但对应 file page 未持久化 → **读到垃圾 / 校验失败 / 静默错**。

## 建议

1. 实现真正的 `sync_data`：per-tbl 跟踪 dirty 数据 / segment fd，fdatasync。
2. `put_at` 提交前按 C++ 顺序：排空页写 → sync 数据 → 写并 fsync manifest。
3. 与 `17-no-append-write-buffer-no-group-commit.md` 一起设计（dirty 位 + 批量 fsync）。

## 验收

- V5 崩溃恢复：kill -9 后 replay 出的 mapping 指向的页全部可读且校验通过。
- 故障注入：在数据页写与 manifest fsync 之间崩溃，重启后不出现悬空引用。
