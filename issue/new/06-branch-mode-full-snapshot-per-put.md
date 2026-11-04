# 06 branch 模式每次 put 写整份 snapshot

**严重度**：P0（写放大）/ P1（性能）

## 现象

`manifest_file.is_some()`（branch 模式）时，每次 `put_at` 走 `write_snapshot_manifest`，把**整份 mapping snapshot** 序列化重写（`crate/engine/fkv_store/src/lib.rs:718-728`，实现 937-962）。

`append_mapping_log` 在 branch 模式下也直接转去写整份 snapshot（lib.rs:806-810）。

另外 `create_branch` 还会额外对 parent 写一次整份 snapshot（lib.rs:1357）。

## C++ 对照

C++ 的 manifest 是**唯一 durability anchor + redo log**：写路径追加 log record（只含 dirty page delta），只有超过 `manifest_limit`（8 MB）或需要切换时才写 snapshot（`src/tasks/write_task.cpp:722-846`）。

branch 写在同一套机制里追加 log，不做整份快照。

## 后果

- branch 写复杂度 O(live pages) 每次 → 分支数据稍大就写不动。
- 与 C++ branch 盘面产生的记录序列不一致，交叉读 / 差分无意义。

## 建议

1. branch 模式复用 `append_mapping_log` 增量路径（dirty page delta）。
2. 只保留「首次 / 超限时切 snapshot」。
3. `create_branch` 去掉多余 parent snapshot。

## 验收

- branch 写一批 1w key，写放大与 main 同量级。
- C++ 能读 Rust 的 branch store，反之亦然。
