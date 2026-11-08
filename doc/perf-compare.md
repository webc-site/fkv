# 与 C++ EloqStore 的性能对标口径

本文记录 `fkv_compare` 每个阶段的采集方式，以及它和 C++ EloqStore / RocksDB 的
一一对应关系。目标很明确：**fkv 的每一项都要接近或超过 C++ 实现**，所以任何
「Rust 侧多出来的东西」都必须指认清楚，不能靠额外缓存或额外线程刷分。

## 1. 数据形状

| 项 | 值 | 说明 |
|---|---|---|
| 行数 | 25 000 000 | 与 C++ 矩阵同规模 |
| 每对宽度 | 116 B（key 16 B + value 100 B） | db_bench 默认形状 |
| key 编码 | 末尾 8 字节大端下标，前导零 | 与 C++ / db_bench 同一编码（字典序 = 下标序） |
| 数据量 | ≈ 2766 MiB | |
| 每引擎缓存 | 256 MiB | 数据 ≈ 10.8 × 缓存，对齐 RocksDB 官方 97 GiB / 6 GiB |

## 2. 缓存对齐（不新增 C++ 没有的缓存）

| fkv | C++ EloqStore | 说明 |
|---|---|---|
| `page_cache_bytes`（index / data 同池，`PageCache`） | `buffer_pool_size`（`PageManager`，同池） | 唯一的一份页缓存，读写共用 |
| `root_meta_cache_bytes` | `root_meta_cache_size` | 映射 / 字典 / 页缓存合计的表级预算 |
| `max_open_tables` | `KvOptions::max_open_tables` 之类的表驱逐 | 超限按 FIFO 驱逐整表状态 |

除这三项之外，引擎里**没有**额外的热点缓存层：快照（`SnapshotRegistry`）只是
登记活跃读视图用于 GC retained set，不做数据缓存；`PageIo::read_page_ref` 是零拷贝
借用而不是缓存；写路径的 `RouteCursor` 只是批内相邻 key 的路由游标（C++ 的写任务
同样在同一次下降里推进）。压测驱动里的 `--read-window` 属于客户端并发窗口，不是
引擎缓存，且默认口径下与 C++ 的 `--readers` 语义对齐。

## 3. 持久化

| fkv | C++ EloqStore | 说明 |
|---|---|---|
| 每批 `sync_data`（数据文件 fdatasync） | `WriteTask::UpdateMeta` → `IoMgr()->SyncData` | 顺序相同：数据先落盘 |
| 追加 manifest + manifest fdatasync | `FlushManifest` → `IouringMgr::AppendManifest` → `SyncFile` | 两者都是两次同步，都不能关闭 |

因此与 C++ 对比时两侧同步强度一致；与 RocksDB 对比时 RocksDB 用**它自己的默认**
（`WriteOptions.sync=false`，不逐次 fsync），这一行是持久化强度差异，页面上单列
说明，不混进「实现优劣」。

## 4. 并发

| 阶段 | fkv | C++ EloqStore |
|---|---|---|
| 点读 | `--readers N` 驱动 N 个客户端线程；引擎侧线程数 = `min(N, 4)`，缓存总量按线程平分，每线程 `FuturesUnordered` 保持 16 个在飞 | `num_threads` 个 shard 线程 + N 个客户端读线程（单分区时 `num_threads=1`，并发由客户端提供） |
| 范围读 / len / 删除 / 写 | 单线程顺序提交 | 单线程顺序提交 |

`--fkv-driver runtime` 是「部署形态」对照：一个进程 N 个 shard 线程、请求按 key
路由，和 C++ 的公开读写接口同层。默认矩阵用 `--fkv-driver store`（进程内直连），
与 C++ 侧的 Store 层同层。

## 5. 结果

数字由 `perf.yml` 每次跑 3 轮取中位数，产物是 `fkv-compare.json`，页面由
`scripts/perf/fkv_compare_to_js.py` 回填。与 C++ 的逐行对比记录在
`doc/architecture/06-gaps.md` 的「性能」小节和 CI artifact 里。
