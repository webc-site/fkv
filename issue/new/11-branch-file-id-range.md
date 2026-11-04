# 11 P0：branch 文件 ID 预留 1024，和 C++ 的“下一个 file id”不一致

## 现象

`fkv_store` 定义：

```rust
// crate/engine/fkv_store/src/lib.rs:66
pub const BRANCH_FILE_RESERVE_FILE_NUM: u64 = 1024;
```

创建 branch 时：

```rust
let branch_base_file = next_file_id
  .div_ceil(BRANCH_FILE_RESERVE_FILE_NUM)
  .checked_mul(BRANCH_FILE_RESERVE_FILE_NUM)?;
let branch_limit_file = branch_base_file + BRANCH_FILE_RESERVE_FILE_NUM;
```

即 branch 从 1024 对齐后的块开始，并预留整个 1024 file id 块（segment 同理）。

## C++ 对照

`src/tasks/background_write.cpp` 的 `CreateBranch`：

```cpp
FileId parent_branch_max_file_id =
    meta->mapper_->FilePgAllocator()->CurrentFileId();
FilePageId new_max_fp_id =
    static_cast<FilePageId>(parent_branch_max_file_id + 1)
    << Options()->pages_per_file_shift;
```

C++ branch 只跳到父 branch 的下一个 file id，不预留 1024。

## 影响

- 同一 parent 下创建的 branch，文件 ID 与 C++ 完全不同
- branch manifest 的 `max_file_id` / segment range 不同
- 跨 C++/Rust 的 branch fixture 无法对齐
- 每个 branch 浪费最多 1024 个 file id；大量 branch 可能提前触发上限
- GC 的 `FileIdInBranch` 范围判断语义不同

## 建议

1. 删除 1024 对齐，按 C++ 规则：
   - data base file = parent current file id + 1
   - segment base file 同样取下一个 segment file id
2. 保留 `branch_file_limit` 作为安全上限，但默认值应等于 base + 1 或由配置决定。
3. 更新 branch 并发隔离、GC、reopen 测试中的预期 file id。
4. 增加 C++ branch fixture 对照。

## 验收

- 同一 parent 创建 branch 后，branch 的首个 data 文件 id 与 C++ 一致
- branch 可写入超过 1024 个文件（或按配置明确上限）
- branch GC 只删除本 branch 范围
