# 16 P1：manifest rename 后没有目录 fsync

## 现象

`CompioStore::switch_manifest`：

```rust
let mut file = File::create(&tmp).await?;
file.write_at(record.to_vec(), 0).await?;
file.sync_data().await?;
rename(&tmp, &path).await?;
```

`append_manifest` 只对文件 `sync_data`，也不 sync 父目录。

## C++ 对照

C++ 的 `WriteTask::AtomicWriteFile` 有明确的 kill point：

- `AtomicWriteFile:Sync`
- `AtomicWriteFile:Rename`
- `AtomicWriteFile:SyncDir`

即写临时文件、fsync、rename、fsync 父目录，保证 crash 后目录项可见。

## 影响

- 机器掉电/内核崩溃后，rename 可能未持久化
- 可能出现 tmp 文件存在但 active manifest 仍指向旧记录
- 与 C++ 的 durability 语义不一致

## 建议

1. `switch_manifest` 在 rename 后打开/复用 table 目录 fd，执行 `fsync`。
2. `append_manifest` 首次创建文件/目录时同样 sync 目录。
3. 与 failpoint 结合测试每个 crash 点。
4. 保持“data/segment 先落盘，manifest 最后安装”的顺序不变。

## 验收

- 在 rename 前/后注入 crash，重启后要么旧 manifest 完整，要么新 manifest 完整
- 目录 fsync 计数可见（metrics 或 tracing）
