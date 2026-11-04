# 04 读路径默认过滤过期项，C++ 不过滤

**严重度**：P0（语义分歧）

## 现象

`Store` 字段 `ttl_read_filter` 默认 `true`（`crate/engine/fkv_store/src/lib.rs:147`），`get_at` 里对 `expire_ts <= now` 的 entry 直接当不存在返回。

代码注释自己也承认：`C++ eloqstore 的 Read / Scan / Floor 不过滤，过期项由 CleanExpired 处理`（lib.rs:158-160）。

## C++ 对照

- Read / Scan / Floor 返回原始 entry，不过滤过期（`src/tasks/read_task.cpp`、`scan_task.cpp`）。
- 过期由 `CleanExpired` 后台任务删除。

## 后果

- 同一 key 在 TTL 到期前后，Rust 读 `None`、C++ 读到旧值 → 操作结果不一致，V3 差分过不了。
- 语义上把「逻辑过期」和「物理删除」耦合了，后续接 C++ 的 CleanExpired 语义会冲突。

## 建议

1. 默认关掉读过滤，或把过滤放到 SDK 层显式开关（对齐 C++ 需要默认 off）。
2. 保留 `clean_expired` 作为唯一的物理删除路径。
3. 补 TTL 到期边界用例，与 C++ 行为对齐。

## 验收

- 同一 TTL 数据，Rust / C++ 在同一时刻读到相同结果。
