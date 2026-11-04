# fkv / eloqstore 差异审查索引

审查对象：

- `fkv` 当前工作区（88 crates，184 tests）
- `eloqstore` baseline `82b774c`
- 本地 `compio` 源码 `0.20.0-beta.1` / `compio-driver 0.13.0-beta.1`
- fkv 实际依赖 `compio 0.19.2` / driver `0.12.5`

目标：找出与 C++ 实现的行为/格式差异，以及 compio 使用上的性能与正确性问题。

优先级：

| 优先级 | 文件 | 一句话 |
|---|---|---|
| P0 | [01-cpp-oracle-and-fixtures.md](./01-cpp-oracle-and-fixtures.md) | 没有真正的 C++ 差分/交叉读写，字节兼容未被证明 |
| P0 | [02-index-page-residency.md](./02-index-page-residency.md) | 索引页没有常驻内存，点读每层都落盘 IO |
| P0 | [03-shard-request-concurrency.md](./03-shard-request-concurrency.md) | shard 串行 await 每个请求，无法交错 IO |
| P0 | [09-overflow-page-path.md](./09-overflow-page-path.md) | Overflow 页只有 codec，读写路径未实现 |
| P0 | [10-cloud-object-layout-upload.md](./10-cloud-object-layout-upload.md) | 云对象 key 与 C++ 不同，且每次整文件上传 |
| P0 | [11-branch-file-id-range.md](./11-branch-file-id-range.md) | branch 文件 ID 预留 1024，C++ 只跳到下一个 file id |
| P0 | [12-runtime-modes-api.md](./12-runtime-modes-api.md) | Runtime 只接 Local，Cloud/Standby 与多个请求类型未暴露 |
| P1 | [04-fd-cache.md](./04-fd-cache.md) | 每次 page IO 重新 open，没有 FD cache |
| P1 | [05-page-io-buffers-validation.md](./05-page-io-buffers-validation.md) | page 读写分配+拷贝，短读写/checksum 未校验 |
| P1 | [06-sync-directory-scans.md](./06-sync-directory-scans.md) | 异步路径里使用同步 read_dir/metadata |
| P1 | [07-scan-prefetch.md](./07-scan-prefetch.md) | Scan 逐页串行读取，没有 prefetch/batch |
| P1 | [08-compression-training-config.md](./08-compression-training-config.md) | 字典压缩需要手工 set_dictionary，没有自动训练和配置接线 |
| P1 | [13-cache-rootmeta.md](./13-cache-rootmeta.md) | fkv_cache / fkv_root 未接入，Store 状态无上限 |
| P1 | [14-ttl-cleanup.md](./14-ttl-cleanup.md) | CleanExpired 扫描整棵 TTL 树，Runtime 无请求入口 |
| P1 | [15-large-value-get.md](./15-large-value-get.md) | get/read 对 large value 返回编码指针而不是值/明确错误 |
| P1 | [16-manifest-durability.md](./16-manifest-durability.md) | manifest rename 后没有目录 fsync |
| P1 | [17-crash-fuzz-failpoints.md](./17-crash-fuzz-failpoints.md) | crash harness / fuzz / failpoint 仍是骨架 |
| P1 | [20-write-batching.md](./20-write-batching.md) | 没有 write buffering / group commit，每个 put 独立 fsync |
| P2 | [18-cross-platform.md](./18-cross-platform.md) | Windows rename、表名大小写/非法字符未处理 |
| P2 | [19-fixed-io-coupling.md](./19-fixed-io-coupling.md) | fixed-buffer 依赖私有 syscall/opcode，维护成本高 |

建议顺序：

1. 先做 01（oracle），否则后续所有“对齐”都无法验收。
2. 并行做 02/03/09/11（核心架构与格式）。
3. 再做 10/12（云与运行模式）。
4. 最后做 IO 性能与可靠性项。
