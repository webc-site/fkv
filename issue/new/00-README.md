# fkv Code Review 意见索引

审查对象：`./fkv`（Rust / compio）对比 `../eloqstore`（C++ / io_uring）。
基线：`COMPAT_BASELINE` = L1（Local + append + 无压缩）。

**一句话结论**

- 格式层（page / manifest / mapping / codec / large）字节布局基本对齐，能对上。
- 引擎热路径和 C++ 差距大：无 page cache、无 fd cache、无 O_DIRECT、无注册 buffer、无批量 IO、无 append 写缓冲。
- 写路径有双重下降 + clone 风暴，读路径每层都从盘读 index page。
- 持久性：数据页从不 fsync，`TermIo` 是桩。
- 结构：6 个 crate 是空壳（`fkv_task` 任务抽象层整体缺失），5 个 crate 无人调用。

**优先级**

- P0：正确性 / 盘面兼容硬伤，不修会数据错或读不了 C++ 的盘。
- P1：性能主路径，决定能否对标 eloqstore。
- P2：架构 / 后续里程碑。

## P0 兼容与正确性

| 文件 | 观点 |
|------|------|
| `01-overflow-page-unused.md` | overflow 页编解码齐全但热路径从不产生/消费 |
| `02-checksum-not-verified.md` | 读路径从不校验 checksum，选项未接线 |
| `03-dict-compression-unwired.md` | 字典压缩整体死代码，manifest 里 dict 永远空 |
| `04-ttl-read-filter-divergence.md` | 读路径默认过滤过期项，C++ 不过滤 |
| `05-split-heuristic-divergence.md` | 0.10 fill ratio + 全量 index 重建 vs C++ 1/4、3/4 重分配 |
| `06-branch-mode-full-snapshot-per-put.md` | branch 模式每次 put 写整份 snapshot，O(live pages) |
| `07-format-bytes-need-cpp-fixture-diff.md` | 几处字节布局未做 C++ 差分，必须补 fixture |
| `08-data-pages-never-fsynced.md` | 数据页从不 fsync，manifest 先于数据落盘 |
| `09-manifest-rename-missing-dir-fsync.md` | rename 后不 fsync 父目录 |
| `10-term-io-stub.md` | `TermIo` 是桩，term CAS 全链路不可用 |
| `11-ensure-table-ready-swallows-missing.md` | 不存在的表被当空表 |

## P1 性能

| 文件 | 观点 |
|------|------|
| `12-sequential-request-handling.md` | shard 串行处理请求，一个大 scan 阻塞后续全部 |
| `13-no-cow-read-snapshot.md` | 读用当前 mapper，无版本化快照 |
| `14-fd-per-io-no-lru.md` | 每次 IO 打开文件，无 fd cache |
| `15-page-io-no-direct-no-registered-buffer.md` | 无 O_DIRECT / 无注册 buffer，读分配 + 两次拷贝 |
| `16-no-io-batching-prefetch.md` | 单次 await，无批量 / 预取，批量常量未用 |
| `17-no-append-write-buffer-no-group-commit.md` | 无 append 写缓冲 / group commit |
| `18-no-page-cache.md` | `fkv_cache` 未接线，index page 每层都从盘读 |
| `19-index-seek-linear.md` | `seek_index` 线性扫，无 restart 二分 |
| `20-write-path-double-descent-clone-storm.md` | 写路径双重下降 + 每 entry 3 次 clone |
| `21-mappingtbl-flat-vec.md` | mapping 是扁平 `Vec<u64>`，COW 快照整份复制 |
| `22-floor-two-pass-and-decompress-alloc.md` | floor 两遍全扫；decompress 对 None 也分配 |

## P2 架构 / 云 / 后续

| 文件 | 观点 |
|------|------|
| `23-cloud-not-wired-whole-file-io.md` | `fkv_cloud` 整文件上传下载，且未接入 runtime |
| `24-standby-prewarm-orphan.md` | `fkv_standby` / `fkv_task_prewarm` 未接入 |
| `25-stub-crates-missing-task-layer.md` | 6 个空壳 crate，`fkv_task` 抽象层缺失 |
| `26-orphan-crates.md` | 5 个 crate 无生产调用者 |
| `27-no-background-compaction-gc.md` | 无后台 compaction / GC，amplify 触发未实现 |
| `28-runtime-request-lock-and-archive-blockon.md` | 每次 request 加锁 + clone；archive crond 阻塞桥接 |

每个文件统一结构：严重度 / 现象 / 证据 / C++ 对照 / 建议 / 验收。
