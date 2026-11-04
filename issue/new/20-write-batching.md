# 20 P1：没有 write buffering / group commit

## 现象

当前每次 `put`：

1. 写 data page（可能多个）
2. 写 TTL page
3. append manifest log
4. `sync_data` manifest 文件

`fkv_shard` 一次只处理一个请求，因此并发写入无法合并；每个 put 都独立触发 page IO + manifest fsync。

## C++ 对照

eloqstore 有 `write_buffer_aggregator` 和 append mode：

- `write_buffer_size` / `write_buffer_ratio` 控制写缓冲
- 并发写可以合并成更大的 page/file 写入
- manifest 提交和 data flush 有独立的批处理节奏

## 影响

- 小写入吞吐受 fsync 次数限制
- 每次 put 都产生 manifest log 记录，放大 manifest 体积
- 与 03（shard 串行）叠加，无法通过并发摊薄 fsync 成本

## 建议

1. 按 table 增加 write aggregator：
   - 收若干 queued put
   - 合并为一次 `write_batch`
   - 一次 manifest append / 一次 fsync
2. 引入 group commit：多个请求共享一次 manifest sync，完成后统一唤醒。
3. 保持单 table 写顺序与 timestamp/term 过滤语义。
4. 将平均 batch size、fsync 次数、写放大纳入 metrics。

## 验收

- 1000 个单 key put 的 manifest fsync 次数明显小于 1000
- 吞吐随并发写入上升
- 崩溃后已确认的 put 不丢失
