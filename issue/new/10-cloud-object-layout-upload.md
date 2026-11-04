# 10 P0：云对象 key 与 C++ 不同，且写路径整文件上传

## 现象

`CloudStore` 的 key 布局：

```rust
// crate/engine/fkv_cloud/src/lib.rs:120
format!("{}/{}/data/", prefix, tbl)
format!("{}/{}", data_prefix, file_id)
format!("{}/{}/manifest/", prefix, tbl)
format!("{}/{}/segment/", prefix, tbl)
```

即：

```
<prefix>/<table>.<partition>/data/<file_id>
<prefix>/<table>.<partition>/manifest/manifest_<branch>_<term>
<prefix>/<table>.<partition>/segment/<file_id>
```

`file_key` / `segment_key` 不包含 branch/term。

`sync_data` 会调用 `flush_tbl`，对每个 dirty file 读取整个本地文件并 `remote.put`：

```rust
// crate/engine/fkv_cloud/src/lib.rs:293
let bytes = self.local.read_file(tbl, file_id).await?;
self.remote.put(&self.file_key(tbl, file_id), &bytes, None, false).await?;
```

## C++ 对照

C++ `AsyncHttpManager::ComposeKey`：

```
[cloud_path prefix/] <table>.<partition> / <filename>
```

其中 `<filename>` 就是 `data_<id>_<branch>_<term>` / `segment_...` / `manifest_<branch>_<term>`，没有 `data/`、`manifest/`、`segment/` 子目录。

C++ 写云时 `WriteTask::UploadState` 只上传本次追加/修改的区间，不是整文件。

## 影响

1. **无法直接挂载 C++ 使用的 bucket**：所有 key 都对不上。
2. **branch/term 冲突**：同一 table 的 main 与 branch、不同 term 的 remote data 都写同一个 `data/<file_id>` key，互相覆盖。
3. **写放大严重**：一个 4 KiB page 写入可能触发 1 GiB data file 全量上传。
4. 远端 GC/list 的 prefix 逻辑也跟着错。

## 建议

1. 改成 C++ 布局：
   - `<prefix>/<table>.<partition>/<filename>`
   - filename 使用 `data_<id>_<branch>_<term>` / `segment_...` / `manifest_...`
2. 远端 data/segment 上传改为增量：
   - S3 不支持原地追加，可复用 C++ 的 chunk/对象切分策略
   - 至少按 dirty page 区间生成新对象或 multipart part，而不是整文件
3. local cache 与 remote key 映射独立，不要用 file_id 直接当对象名。
4. 增加 C++ 云 fixture / RustFS 对照。

## 验收

- Rust 能读取 C++ 写入的 cloud bucket
- branch 与 main 的 remote 对象不冲突
- 单页写入上传字节数与 dirty 区间同阶，而不是整个 file
