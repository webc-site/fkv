# 03 字典压缩整体是死代码，manifest 里 dict 永远空

**严重度**：P0（盘面不完整）/ P1（性能）

## 现象

`fkv_compress` 里字典路径写完了（`DictCompression::maybe_build_dictionary` / `build_dictionary` / `compress` / `decompress`），但引擎从不调用：

- 每次读传 `dict: None`。
- 每次写 manifest 的 `dict_bytes: b""`（`crate/engine/fkv_store/src/lib.rs:345、421、490、720、830、1065、1110、1159`）。
- `maybe_build_dictionary` 无调用者。
- 只有孤立的 `fkv_root` 会加载字典。

结果：**只有 standalone / raw zstd 路径是活的**，`CompressionType::Dictionary` 永不被写出。

## C++ 对照

- 写路径 `BatchWriteTask::Apply` 调 `compression_->SampleAndBuildDictionaryIfNeeded(batch)`（`src/tasks/batch_write_task.cpp:251-274`）。
- 字典存进 manifest snapshot 的 `dict_bytes`（`src/storage/root_meta.cpp:84-132`）。
- 阈值见 `include/kv_options.h`：`enable_compression` 默认 false；字典 max 16 KiB。

## 后果

- 盘面缺 dictionary 分支，短值（100B ~ 10MiB）压缩率远低于 C++。
- 读 C++ 写的 `CompressionType::Dictionary` 值会失败：`dict` 为 None → `KvError::Corrupted`（`fkv_btree/src/lib.rs:87-90`）。

## 建议

1. 写路径接入 `maybe_build_dictionary`，样本累计 + 训练，把字典写进 snapshot 的 `dict_bytes`。
2. 读路径从 manifest 载入字典并传给 `decompress_value`。
3. `fkv_root` 的 RootMetaMgr 接管每表字典缓存（现为孤儿 crate）。
4. 补差分：同一批值，Rust / C++ 字典压缩结果结构等价。

## 验收

- 写入触发字典训练；manifest 里 `dict_len > 0`。
- C++ 写的字典压缩值 Rust 能解，反之亦然。
