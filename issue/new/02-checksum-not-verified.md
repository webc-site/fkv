# 02 读路径从不校验 checksum

**严重度**：P0（正确性 / 兼容行为）

## 现象

- 写入时算了 checksum：`set_checksum`（`crate/format/fkv_page/src/header.rs:78-86`），范围与 C++ 一致（整页含 padding，跳过前 8B，`fkv_codec::checksum_excluding_prefix`）。
- `validate_checksum`（header.rs:88-91）和 manifest 的 `validate_record`（`fkv_manifest/src/lib.rs:309`）**只在测试里被调用**。
- 引擎读路径（`read_page` / `read_entry` / `descend_to_data`）读完直接解析，从不校验。
- `KvOptions::skip_verify_checksum` 有字段但无人读取。

## C++ 对照

`IouringMgr::ReadPage` / `ReadPages` 默认对每页做 XXH3-64 校验，仅当 `skip_verify_checksum=true` 才跳过（`src/async_io_manager.cpp:783-788`、`:792-931`；`include/kv_options.h:47`）。

## 后果

- 静默数据损坏（bit rot / 半写页）会被当成正常数据返回，产生错误结果而非报错。
- 与 C++ 行为不一致，无法用同一份 fixture 校验损坏检测。

## 建议

1. `read_page` 返回后（或引擎下降每层时）调 `validate_checksum`，失败返回 `KvError::Corrupted`。
2. 接线 `skip_verify_checksum` 选项。
3. 考虑校验开销：可只在校验开关打开时做，或对齐 C++ 的批量读后统一校验。

## 验收

- 人为翻转一个页字节，Rust 读到该页返回 `Corrupted`。
- `skip_verify_checksum=true` 时不报错（行为对齐 C++）。
