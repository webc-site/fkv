# 11 不存在的表被当空表

**严重度**：P0 / P1（语义）

## 现象

`ensure_table_ready` 在 main 模式下把 `Err(KvError::ResourceMissing)` 和 `Ok(())` 同等对待（`crate/engine/fkv_shard/src/lib.rs:234-238`）：

```rust
match state.store.reopen(tbl).await {
    Ok(()) | Err(KvError::ResourceMissing) => {}
    Err(err) => return Err(err),
}
```

即对不存在的表调用 reopen 失败被吞，后续读当空表、写当新表。

## 后果

- 打错表名 / 分区号不会报错，静默创建新分区，问题被掩盖。
- 与 C++ 行为可能不一致：需确认 C++ reopen 不存在的分区到底是报错还是按空表（`src/eloq_store.cpp` 分区打开路径）。

## 建议

1. 先确认 C++ 语义（空表 vs 报错），再对齐。
2. 若确实要「不存在即空表」，加显式注释和测试；否则返回 `ResourceMissing`。

## 验收

- 打开不存在的分区，行为与 C++ 一致（读空 / 报错）。
