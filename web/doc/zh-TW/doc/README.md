# fkv

fkv 是純 Rust 的嵌入式 KV 儲存引擎。

## 特性

- 每 shard 一個 OS 線程，協作式 work loop。
- COW B+tree，append-only segment IO。
- page / index / manifest / overflow / compression 格式。
- local / memory / simulated / S3 物件後端。
- standalone / cloud / standby replica 模式。
- 不做 C ABI，不做 Python SDK；公開介面只有 Rust。

## 入口

- [架構](architecture.md)
- [Crate](crates.md)
- [測試](testing.md)
- [效能](performance.md)