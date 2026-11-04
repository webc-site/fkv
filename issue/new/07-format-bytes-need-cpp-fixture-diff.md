# 07 几处字节布局未做 C++ 差分，必须补 fixture

**严重度**：P0（兼容验证缺口）

## 现象

下列布局 Rust 代码注释都写了「对齐 eloqstore」，但**没有 C++ oracle 差分证据**，靠人眼对照：

1. **branch metadata 序列化**：Rust 为 `u32 len | u32 name_len | name | u64 term | u64 num_entries | (u32 name_len | name | u64 term | u64 max_file_id | u64 max_segment_file_id)*`（`fkv_manifest/src/lib.rs:71-88`）。C++ `BranchManifestMetadata` 外层多一个 `meta_len` 前缀，且 `SerializeBranchFileMapping` 对 `u64` 用 **native memcpy**（非 LE），头注释与实现还不一致（`include/common.h:695-798`）。LE 机器上应当一致，但没验证过。
2. **压缩帧**：Rust 为 `varint32(original_len) | zstd_frame`（`fkv_compress/src/lib.rs:50-59`）。需与 `src/compression.cpp` 逐字节比对。
3. **segment 文件参数**：Rust `pages_per_file_shift=18` 硬编码在 `Store`（`fkv_store/src/lib.rs:143`）；需核对 `segments_per_file_shift`、segment file 大小推导与 C++（`include/kv_options.h:257-261、362-387`）一致。
4. **manifest log 记录**：Rust 靠 `finalize` 补齐到 4096（`fkv_manifest/src/lib.rs:253-264`），需确认与 C++ 每条记录 `page_align` padding 完全一致（`src/replayer.cpp:104-138`）。

## 建议

1. 用 `fkv_ledger` 的 `fixture` 子命令（现未实现）生成 C++ 侧样本：page / manifest log / manifest snapshot / 压缩帧 / branch metadata。
2. 逐字节 diff：固定布局要求逐字节相等；zstd / 文件 ID 结构等价。
3. 把这些做成 CI 门槛（V2）。

## 验收

- 每个格式 crate 有 `_cpp` 差分用例，字节或结构等价。
- 关掉 `case-insensitive` 之类的人工对照，改为程序化比对。
