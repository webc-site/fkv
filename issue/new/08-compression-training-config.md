# 08 P1：字典压缩需要手工 set_dictionary，没有自动训练与配置接线

## 现象

- `DictCompression::maybe_build_dictionary` 已实现，但生产代码没有任何调用：
  - `crate/format/fkv_compress/src/lib.rs`
  - `grep maybe_build_dictionary crate` 只命中测试
- Store 只提供 `with_compression_level` 和 `set_dictionary`
- `KvOptions.enable_compression` / `zstd_compression_level` 没有映射到 Store/Runtime/SDK
- 没有字典时，100B..10MiB 的 value 走 `CompressionType::None`，只有 >10MiB 才 standalone
- 现有 `mem_store_compression_roundtrip_*` 测试没有设置字典，也没有断言页面真的带 compression flag

## C++ 对照

`BatchWriteTask::Run` 会调用：

```cpp
cow_meta_.compression_->SampleAndBuildDictionaryIfNeeded(data_batch_);
```

达到 `kSampleTargetBytes` / `kMinSamples` 后训练字典，并写入 manifest 的 `dict_bytes`；后续写入自动使用 `CompressionType::Dictionary`。

## 影响

- 用户打开 `enable_compression` 后，绝大多数 value 实际没有压缩
- 必须手工传入训练好的字典才能得到压缩收益
- 与 C++ 的“自动采样 -> 训练 -> 落 manifest”流程不一致
- 测试可能“通过但没有压缩”，掩盖功能未生效

## 建议

1. 在 Store 写路径（put/put_large 的 inline 分支）调用 `maybe_build_dictionary`。
2. 字典构建成功后：
   - 更新 `compression_map`
   - 在 manifest snapshot 中写 `dict_bytes`
   - 让 branch 继承
3. 把 `KvOptions.enable_compression` / `zstd_compression_level` 接到 SDK/Runtime/Store。
4. 测试断言 `DataPageIter::compression()` 或磁盘页大小，而不是只断言 roundtrip。

## 验收

- 仅设置 `enable_compression=true` 就能产生 Dictionary 压缩页
- reopen 后 manifest 恢复字典
- C++ 能读取 Rust 的字典压缩页，反之亦然
