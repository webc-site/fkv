# 15 P1：get/read 对 large value 返回编码指针

## 现象

`Store::get_at` 只做 TTL 过滤后直接返回 `entry.value`：

```rust
// crate/engine/fkv_store/src/lib.rs:253
Ok(Some(entry.value))
```

它没有检查 `entry.is_large_value`。large value 的 `value` 是 `fkv_large` 编码后的 segment pointer + metadata trailer，不是用户数据。

`read_at` 复用 `get_at`，所以同样受影响。

## C++ 对照

C++ `ReadRequest` 会：

- 识别 large value
- 按 `large_value_dest_` 把数据读入 `std::string` / `IoStringBuffer` / pinned memory
- 或者返回 metadata-only
- 不会把 pointer 编码泄露给调用方

## 影响

- 用户调用 `get`/`read` 可能拿到内部编码，误当数据使用
- API 语义错误，且难以排查
- SDK 虽然有 `get_large`，但通用 get 仍可能被误用

## 建议

1. `get_at` 检测 `is_large_value`：
   - 默认返回 `LargeValueUnsupported`
   - 或新增 `read_value` 自动解析（需 SegmentIo）
2. 提供明确的读取模式：
   - `get_large` → Vec
   - `get_large_into` → fixed buffer
   - metadata-only
3. 增加测试：large value 上调用 `get` 必须返回错误或完整值，绝不能返回 pointer。

## 验收

- `get(large_key)` 不返回内部编码
- C++ Read 的 large-value 目的模式在 Rust API 有对应或明确不支持
