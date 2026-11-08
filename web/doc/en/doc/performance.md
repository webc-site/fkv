# Performance

`fkv_bench` Test write, read, and scan loads on a fixed key space.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indicators include throughput, latency at each stage, and peak RSS. release workflow release x86_64 /
`fkv_bench` binary for aarch64.

## Three-way matchup with RocksDB/fjall

`crate/perf/fkv_compare` Use the same stage driver and the same key encoding (8-byte big-endian subscript at the end,
Dictionary order is consistent with subscript order) Test fkv, RocksDB, fjall: RocksDB passed
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` The compiled shared library is loaded with dlopen (the same C
ABI, Rust build does not link to RocksDB), fjall uses `fjall 3` directly. Each of the three engines opens a library.
Advance on the same library in the order of stages: bulk load creates data, and then len / point reading / range reading /
Deletions all affect the same data.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Skip the third column if there is no RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

The numbers for the homepage matrix can only come from one traceable run:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI run <id>"
```

GitHub Actions' `perf.yml` does exactly this (install `librocksdb-dev`, program adapters, run
25M row matrix, upload artifact, and directly backfill the page when `backfill=true`).

### Caliber

- **Data/Cache Ratio**: 25,000,000 rows × 116 B ≈ 2766 MiB of data, 256 MiB of cache per engine,
  ≈ 11:1, aligned with RocksDB’s official review of 97 GiB data/6 GiB cache (≈ 16:1).
- **Same amount of cache**: fkv’s page cache (index/data same pool), RocksDB’s block cache
  (`cache_index_and_filter_blocks=true`, block size 4 KiB), fjall's own block cache
  The budgets are all set to the same number of bytes.
- **Persistence caliber (the comparison column uses the RocksDB default configuration, without alignment)**: fkv 1×data page + per batch
  1×manifest `fdatasync` (Isomorphic to SyncData + AppendManifest of C++ EloqStore,
  Neither has an off switch); the RocksDB column uses the **default configuration** (WAL on, `WriteOptions.sync=false`,
  No sequential fsync), fjall also uses the default (`PersistMode::Buffer`). Therefore the write phase (single write /
  The difference between batch writes) is the **persistence strength difference**, not the implementation difference, which must be stated on the page along with the number.
  If you want to analyze "who is faster under the same synchronization strength", add `--align-fsync` (open RocksDB with sync=true,
  fjall uses SyncAll), this option is not used for externally published matrices.
- **Two fkv drivers**: `--fkv-driver store` (default, directly connected to a single `Store` within the process, and
  RocksDB / fjall are also library calls) and `--fkv-driver runtime` (deployment mode: a process
  `--fkv-shards` shard threads, requests are routed by key, aligned with public reading and writing of C++ EloqStore
  interface). Each request of the latter needs to go through the queue + cross-thread wake-up, native (macOS, file IO goes through the thread pool)
  Reading time is about 20 µs/time, which is an order of magnitude slower than in-process direct connection; much cheaper on Linux/io_uring, but
  Still a jump. The page matrix uses the store caliber, and the runtime caliber is used for "deployment form vs C++
  Comparison of "Public Interface" (fkv runtime single-thread reading is about 50 kops/s, C++ EloqStore FFI single-thread
  shard about 21 kops/s).
- **Read concurrency caliber**: fkv side thread number = `min(readers, --read-max-concurrency)`, default
  The upper limit is 4 (aligned with the evaluation machine vCPU), **not** "one thread per reader" - the exceeded level only increases the queue;
  Cache per thread = 256 MiB / number of threads (the total amount is fixed, not shredded with readers), and used
  `FuturesUnordered` Keep `--read-window` (default 16) requests on the fly. RocksDB side is
  `readers` Each `std::thread` reads simultaneously; single-threaded loop on fjall side (ignoring readers gear,
  As a reference column only). `--no-fkv-read-threads` can degenerate back to "interleaved N asynchronous chains on a single runtime".
  That's pipeline depth, not concurrency, and is not comparable across engines.
- **len()**: None of the three engines have O(1) counting interfaces. This line is the full table counting rate, not O(1) lookup.