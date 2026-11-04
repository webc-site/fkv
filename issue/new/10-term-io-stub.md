# 10 TermIo 是桩，term CAS 全链路不可用

**严重度**：P0（功能缺失）

## 现象

`CompioStore` 的 `TermIo`：`read_term` 恒返回 `Ok(None)`，`upsert_term` 恒返回 `Err(KvError::NotRunning)`（`crate/io/fkv_io_compio/src/lib.rs:502-510`）。

上游影响：

- `Store::claim_term` 永远失败。
- `put_with_term` 在本地后端只在 `term == 0` 时能成功。
- `fkv_shard` 对 Put / PutLarge 校验 `state.term`（`fkv_shard/src/lib.rs:299、311`），但没有可用的 term 存储。

## C++ 对照

term 文件 `CURRENT_TERM_<branch>_<pgid>`，`UpsertTerm` 用条件写实现 CAS（`src/async_io_manager.cpp`、`src/storage/object_store.cpp`），Standby / Cloud 靠它做单 writer 仲裁。

## 后果

Local / Cloud 模式的 term 协商、standby 切换的全部前提缺失。

## 建议

1. 实现本地 term 文件读写 + CAS（对齐 C++ 文件名与内容格式）。
2. 与 `fkv_standby` 接入一起验证。

## 验收

- `claim_term` 单调递增，并发两个 writer 只有一个成功。
- term 文件字节格式与 C++ 一致。
