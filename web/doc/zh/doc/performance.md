# 性能

`fkv_bench` 在固定 key 空间上测写、读、scan 负载。

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

指标含吞吐、各阶段延迟分位与峰值 RSS。release workflow 发布 x86_64 /
aarch64 的 `fkv_bench` 二进制。

## 与 RocksDB / fjall 的三方对拍

`crate/perf/fkv_compare` 用同一份阶段驱动、同一条 key 编码（末尾 8 字节大端下标，
字典序与下标序一致）测 fkv、RocksDB、fjall：RocksDB 通过
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` 编出的共享库以 dlopen 加载（同一 C
ABI，Rust 构建不链接 RocksDB），fjall 直接用 `fjall 3`。三个引擎各开一个库，
按阶段顺序在同一个库上推进：bulk load 造数据，之后的 len / 点读 / 范围读 /
删除都作用在同一份数据上。

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh            # 没有 RocksDB 就跳过第三列
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

首页矩阵的数字只能来自一次可追溯的运行：

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI 运行 <id>"
```

GitHub Actions 的 `perf.yml` 做的就是这件事（装 `librocksdb-dev`、编适配器、跑
25M 行矩阵、上传 artifact，并在 `backfill=true` 时直接回填页面）。

### 口径

- **数据 / 缓存比**：25,000,000 行 × 116 B ≈ 2766 MiB 数据，每引擎 256 MiB 缓存，
  ≈ 11:1，对齐 RocksDB 官方评测的 97 GiB 数据 / 6 GiB 缓存（≈ 16:1）。
- **缓存同额**：fkv 的页缓存（index / data 同池）、RocksDB 的 block cache
  （`cache_index_and_filter_blocks=true`，block size 4 KiB）、fjall 自己的块缓存
  预算都设成同一份字节数。
- **持久化口径（对照列用 RocksDB 默认配置，不做对齐）**：fkv 每批 1×数据页 +
  1×manifest `fdatasync`（与 C++ EloqStore 的 SyncData + AppendManifest 同构，
  两者都没有关闭开关）；RocksDB 列用**默认配置**（WAL 开、`WriteOptions.sync=false`，
  不逐次 fsync），fjall 同样用默认（`PersistMode::Buffer`）。因此写阶段（单条写入 /
  批量写）的差距是**持久化强度差异**，不是实现差异，页面上必须连同数字一起说明。
  想分析「同样同步强度下谁快」，加 `--align-fsync`（把 RocksDB 打开 sync=true、
  fjall 用 SyncAll），该选项不用于对外发布的矩阵。
- **两种 fkv 驱动**：`--fkv-driver store`（默认，进程内直连单个 `Store`，和
  RocksDB / fjall 一样是库调用）与 `--fkv-driver runtime`（部署形态：一个进程
  `--fkv-shards` 个 shard 线程，请求按 key 路由，对齐 C++ EloqStore 的公开读写
  接口）。后者的每次请求要过队列 + 跨线程唤醒，本机（macOS，文件 IO 走线程池）
  上点读约 20 µs/次，比进程内直连慢一个数量级；Linux/io_uring 上便宜得多，但
  仍是一次跳转。页面矩阵用的是 store 口径，runtime 口径用于「部署形态 vs C++
  公开接口」的对照（fkv runtime 单线程点读约 50 kops/s，C++ EloqStore FFI 单
  shard 约 21 kops/s）。
- **点读并发口径**：fkv 侧线程数 = `min(readers, --read-max-concurrency)`，默认
  上限 4（与评测机 vCPU 对齐），**不是**「每读者一个线程」——超出的档位只增加排队；
  每线程缓存 = 256 MiB / 线程数（总量固定，不随 readers 切碎），并用
  `FuturesUnordered` 保持 `--read-window`（默认 16）个在飞请求。RocksDB 侧是
  `readers` 个 `std::thread` 各自同步读；fjall 侧单线程循环（忽略 readers 档位，
  仅作为参考列）。`--no-fkv-read-threads` 可退化回「单 runtime 上 N 条异步链交错」，
  那是流水线深度而不是并发度，跨引擎不可比。
- **len()**：三个引擎都没有 O(1) 计数接口，这一行是全表计数速率，不是 O(1) 查找。
