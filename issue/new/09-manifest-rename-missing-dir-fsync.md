# 09 manifest rename 后不 fsync 父目录

**严重度**：P0（崩溃一致性）

## 现象

`CompioStore::switch_manifest`：写 tmp → `sync_data` → `rename(tmp, path)`，**之后没有 fsync 父目录**（`crate/io/fkv_io_compio/src/lib.rs:423-432`）。

`write_manifest_file` 同样（lib.rs:450-459）。

## C++ 对照

`SwitchManifest` / `WriteSnapshot`：tmp 文件 → fdatasync → rename → **fdatasync 目录**（`src/async_io_manager.cpp:2887-2965`）。

## 后果

rename 本身是原子的，但目录项落盘没有保证。断电后可能出现：新 manifest 文件内容在盘上，但目录项回退到旧名 / 丢失 → 打开 store 读到旧 manifest 或找不到。

## 建议

1. rename 后打开父目录 fd，`sync_data`（或 `File::open(dir).sync_all()`）。
2. 覆盖 switch_manifest / write_manifest_file / 以及 GC 删除文件后的目录同步。

## 验收

- 在 rename 与 dir-fsync 之间注入崩溃，重启后 manifest 状态一致（要么旧要么新，不出现半状态）。
