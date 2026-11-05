# fkv

fkv 是纯 Rust 的嵌入式 KV 存储引擎。

## 特性

- 每 shard 一个 OS 线程，协作式 work loop。
- COW B+tree，append-only segment IO。
- page / index / manifest / overflow / compression 格式。
- local / memory / simulated / S3 对象后端。
- standalone / cloud / standby replica 模式。
- 不做 C ABI，不做 Python SDK；公开接口只有 Rust。

## 入口

- [架构](architecture.md)
- [Crate](crates.md)
- [测试](testing.md)
- [性能](performance.md)
