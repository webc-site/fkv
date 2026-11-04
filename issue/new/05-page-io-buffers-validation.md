# 05 P1：page IO 分配+拷贝，短读写与 checksum 未校验

## 现象

`read_page`：

```rust
// crate/io/fkv_io_compio/src/lib.rs:357
let BufResult(result, buf) = file
  .read_at(vec![0u8; self.page_size], offset).await;
result.map_err(|_| KvError::IoFail)?;
dst[..self.page_size].copy_from_slice(&buf[..self.page_size]);
```

- 每次读都新分配一个 `page_size` Vec
- 读结果长度被忽略（短读也按满页 copy）
- 读路径没有 `validate_checksum`

`write_page`：

```rust
let BufResult(result, _) = file.write_at(src.to_vec(), offset).await;
```

- 每次写都复制一份
- 没有检查实际写入长度

## C++ 对照

- 读 page 后检查 `res == data_page_size`，再 `ValidateChecksum`
- `skip_verify_checksum` 才跳过
- page 缓冲来自 PageManager/注册内存，避免 per-IO 分配

## 影响

- 正确性：短读/损坏 page 会被当作零填充数据，直到解析失败或返回错误值
- 性能：每页一次 alloc + memcpy；scan 时尤其明显
- 无法通过 `skip_verify_checksum` 配置控制

## 建议

1. 复用 per-shard page buffer（PagePool/PageManager），至少消除每 IO 分配。
2. 检查 `read_at` 返回长度必须等于 `page_size`。
3. 读后调用 `validate_checksum`，支持 `skip_verify_checksum`。
4. 写后检查写入长度等于期望。
5. 中期把 page buffer 纳入 cache/注册内存，向 C++ 靠拢。

## 验收

- 截断 page fixture 读取返回 `Corrupted`
- checksum 破坏 fixture 读取失败（skip=false）
- 10k 次 page read 的分配次数接近 0（或只来自缓存）
