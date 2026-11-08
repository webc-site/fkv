# 效能

`fkv_bench` 在固定 key 空間上測寫、讀、scan 負載。

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

指標含吞吐、各階段延遲分位與峰值 RSS。 release workflow 發布 x86_64 /
aarch64 的 `fkv_bench` 二進位。

## 與 RocksDB / fjall 的三方對拍

`crate/perf/fkv_compare` 以同一階段驅動、同一 key 編碼（結尾 8 位元組大端下標，
字典序與下標序一致）測 fkv、RocksDB、fjall：RocksDB 透過
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` 編出的共享庫以 dlopen 載入（同一 C
ABI，Rust 建構不連結 RocksDB），fjall 直接用 `fjall 3`。三個引擎各開一個庫，
依階段順序在同一個函式庫上推進：bulk load 造數據，之後的 len / 點讀 / 範圍讀 /
刪除都會作用在同一份資料上。

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # 沒有 RocksDB 就跳過第三列
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

首頁矩陣的數字只能來自一次可追溯的運行：

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI 運行 <id>"
```

GitHub Actions 的 `perf.yml` 做的就是這件事（裝 `librocksdb-dev`、編適配器、跑
25M 行矩陣、上傳 artifact，並在 `backfill=true` 時直接回填頁面）。

### 口徑

- **數據 / 快取比**：25,000,000 行 × 116 B ≈ 2766 MiB 數據，每引擎 256 MiB 緩存，
  ≈ 11:1，對齊 RocksDB 官方評測的 97 GiB 資料 / 6 GiB 快取（≈ 16:1）。
- **快取同額**：fkv 的頁快取（index / data 同池）、RocksDB 的 block cache
  （`cache_index_and_filter_blocks=true`，block size 4 KiB）、fjall 自己的區塊緩存
  預算都設成同一份位元組數。
- **持久化口徑（對照列用 RocksDB 預設配置，不做對齊）**：fkv 每批 1×資料頁 +
  1×manifest `fdatasync`（與 C++ EloqStore 的 SyncData + AppendManifest 同構，
  兩者都沒有關閉開關）；RocksDB 列用**預設配置**（WAL 開、`WriteOptions.sync=false`，
  不逐次 fsync），fjall 同樣用預設（`PersistMode::Buffer`）。因此寫階段（單一寫入 /
  批量寫）的差距是**持久化強度差異**，不是實現差異，頁面上必須連同數字一起說明。
  想分析「同樣同步強度誰快」，加 `--align-fsync`（把 RocksDB 打開 sync=true、
  fjall 用 SyncAll），此選項不用於對外發布的矩陣。
- **兩種 fkv 驅動**：`--fkv-driver store`（默認，進程內直連單一 `Store`，和
  RocksDB / fjall 一樣是函式庫呼叫）與 `--fkv-driver runtime`（部署形態：一個行程
  `--fkv-shards` 個 shard 線程，請求按 key 路由，對齊 C++ EloqStore 的公開讀寫
  接口）。後者的每次請求要過佇列 + 跨執行緒喚醒，本機（macOS，檔案 IO 走執行緒池）
  上點讀取約 20 µs/次，比進程內直連慢一個數量級；Linux/io_uring 上便宜得多，但
  仍是一次跳轉。頁面矩陣用的是 store 口徑，runtime 口徑用於「部署形態 vs C++
  公開介面」的對照（fkv runtime 單線程點讀約 50 kops/s，C++ EloqStore FFI 單
  shard 約 21 kops/s）。
- **點讀並發口徑**：fkv 側執行緒數 = `min(readers, --read-max-concurrency)`，預設
  上限 4（與評測機 vCPU 對齊），**不是**“每讀者一個線程”——超出的檔位只增加排隊；
  每執行緒緩存 = 256 MiB / 執行緒數（總量固定，不隨 readers 切碎），並以
  `FuturesUnordered` 保持 `--read-window`（預設 16）個在飛行請求。 RocksDB 側是
  `readers` 個 `std::thread` 各自同步讀取；fjall 側單執行緒迴圈（忽略 readers 檔位，
  僅作為參考列）。 `--no-fkv-read-threads` 可退化回「單 runtime 上 N 條非同步鏈交錯」，
  那是流水線深度而不是並發度，跨引擎不可比擬。
- **len()**：三個引擎都沒有 O(1) 計數接口，這一行是全表計數速率，不是 O(1) 查找。