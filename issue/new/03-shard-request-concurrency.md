# 03 P0：shard 串行 await 每个请求，无法交错 IO

## 现象

`fkv_shard` 的主循环是：

```rust
// crate/engine/fkv_shard/src/lib.rs:154
while let Ok(envelope) = rx.recv().await {
  let result = handle(&mut state, envelope.req).await;
  ...
}
```

`handle` 会完整执行一次 put/get/scan，包括所有 page IO、manifest fsync、TTL 清理，期间不会从队列取下一条请求。

## C++ 对照

eloqstore 的 shard 有 task manager / pending write queue：

- 读任务可以并发在 shard 内推进
- 写任务按 partition 串行，但不同 partition 的 IO 可以交错
- 任务在 IO 点 yield，完成后由 task manager 继续推进

参考 `docs/architecture/04-execution-model.md`。

## 影响

- 一条慢 IO（尤其 scan 或 fsync）阻塞该 shard 的所有 table/请求
- io_uring 的并发深度基本为 1
- 多线程 `num_threads` 只能靠 shard 间并行，单表吞吐受限
- 与 C++ 的异步任务模型差距明显

## 建议

1. 把每个请求包成独立 compio task（`Runtime::spawn` / `spawn_local`），由运行时调度。
2. 按 table 维护写队列：
   - 同一 table 的写串行
   - 不同 table 的写可并行
   - 读不阻塞写队列，但需要和 COW root 切换做顺序保证
3. 保持 attach/detach/shutdown 的屏障语义。
4. 增加 in-flight 上限和队列深度指标。

## 验收

- 单 shard 同时发起 32 个 get，IO 能并发在飞
- 慢 table 的 scan 不阻塞其他 table 的 get
- attach/detach 与并发请求的线性化测试通过
