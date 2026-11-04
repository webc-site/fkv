# 12 P0：Runtime 只接 Local，Cloud/Standby 与一批请求类型未暴露

## 现象

`Shard::start` 固定构造 `CompioStore`：

```rust
// crate/engine/fkv_shard/src/lib.rs:111
CompioStore::new(config.root, config.page_size, config.pages_per_file_shift)
```

- 没有 `CloudStore` / `StandbyService` 的运行时构造
- `RuntimeConfig` 只有 num_threads/root/page_size 等本地字段
- `KvOptions` 的 `store_path`、`cloud_*`、`enable_local_standby` 没有驱动 Runtime
- `fkv_api::Request` 只有 Put/Get/Scan/Reopen/Gc/Compact/Truncate/Floor/Drop/Archive/Branch/Attach/Detach

缺少：

- `CleanExpired`
- `FileGc` / `LocalGc` 区分
- `Prewarm`
- `ListArchiveTags`
- `GlobalArchive` / `GlobalReopen` / `GlobalCreateBranch` / `DropTable` 全局 fanout
- Archive delete action
- `Reopen(tag/clean)`
- `ListObject` / `ListStandbyPartition`
- OOM retry

## C++ 对照

`docs/architecture/03-request-api.md` 列出了完整的请求 taxonomy，包括 remote listing、global fanout、file/local GC、clean expired、prewarm、archive action。C++ Runtime 通过 `StoreMode` / `KvOptions` 选择 Local/Mem/Cloud/Standby。

## 影响

- 库级测试覆盖了 Cloud/Standby，但用户无法通过 Runtime/SDK 使用
- API 对齐度不足，C++ 调用方迁移会卡住
- `CleanExpired`、`Prewarm` 等只能直接调 Store，无法进入 shard 队列
- 云/standby 的 auto-reopen 无法真正生效

## 建议

1. 在 shard 引入 IO backend 枚举：
   - `Local(CompioStore)`
   - `Cloud(CloudStore<CompioStore, S3Store>)`
   - `Standby(...)`
2. `RuntimeConfig` 增加 mode/cloud/standby/prewarm/cache 字段，或直接接受 `KvOptions`。
3. 补齐 `Request` 枚举和 `handle` 分支，先做 per-table，再做 runtime 级 global fanout。
4. 增加与 C++ 请求类型的对照表测试。

## 验收

- Runtime 能以 Cloud/Standby 模式启动并完成 put/get/reopen
- CleanExpired/Prewarm/ListArchiveTags 可通过 Runtime API 调用
- 请求类型覆盖表与 C++ 一致（允许明确标注的未实现项）
