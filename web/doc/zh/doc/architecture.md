# 架构

## 运行时

每个 shard 独占一个 OS 线程与一个 compio runtime。请求经 ingress 队列进入，
由 shard 状态机准入，通过 oneshot 回执完成。

## 存储

引擎使用 copy-on-write B+tree。data page、index page、overflow chain 与
manifest 序列化进 segment 文件。branch 由 `(branch, term)` 标识，各自持有
manifest 与 mapping snapshot。

## IO

`fkv_io` 定义异步 IO trait。`fkv_io_compio` 用 compio + io_uring 实现，
`fkv_io_mem` / `fkv_io_sim` 提供内存与故障注入后端，`fkv_object_s3` 提供
S3 对象后端。

## 持久化

manifest 记录 root page、TTL root、branch metadata 与 mapping。Reopen 时
先 replay manifest，再重建 page mapper，之后才开始服务请求。
