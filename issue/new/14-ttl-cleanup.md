# 14 P1：CleanExpired 扫描整棵 TTL 树，Runtime 无请求入口

## 现象

`Store::clean_expired` 的 scan 范围是空的：

```rust
// crate/engine/fkv_store/src/lib.rs:1206
scan_entries(
  ... begin: &[], end: &[], max_entries: 0)
```

即扫描所有 TTL key，再在内存里过滤 `expire_ts <= now`。

同时：

- `fkv_api::Request` 没有 `CleanExpired`
- `Runtime` 没有 `clean_expired` 方法
- 只有 Store 内部写后触发（`maybe_clean_expired`）和显式调用

## C++ 对照

C++ `CleanExpiredKeys` 在 TTL tree 上扫描 `[0, now]` 范围，而不是全树；由 `TriggerTTL` / 后台任务按 `next_expire_ts_` 调度。

## 影响

- TTL tree 很大时，每次清理都是 O(全部 TTL 条目)
- 写路径的 TriggerTTL 可能突然产生长尾
- 用户无法通过 Runtime 主动触发清理
- 与 C++ 的清理复杂度不一致

## 建议

1. 构造 TTL 范围：
   - begin = `[]`
   - end = `ttl_key(now, &[])` 或 `ttl_key(now, 0xff...)` 的上界，只扫描到期区
2. 扫描到第一个 `expire_ts > now` 即停。
3. 增加 `Request::CleanExpired` 和 `Runtime::clean_expired`。
4. 用 `next_expire_ts` 做后台定时/写后触发，避免每次全树。

## 验收

- 10 万 TTL 条目中只有 100 条过期时，清理扫描条目数接近 100
- Runtime API 可调用
- 清理后 `next_expire_ts` 正确
