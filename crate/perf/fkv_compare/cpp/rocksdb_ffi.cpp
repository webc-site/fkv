// RocksDB 版本的 benchmark FFI：与 `bench_ffi.cpp` 共用同一 C ABI（bench_ffi.h），
// 矩阵把两个 .so 当同类引擎加载，第三个引擎列因此不需要改矩阵的调用协议。
//
// 缓存 / 持久化配置刻意与 fkv、EloqStore 对齐（用户要求"各种缓存配置对齐"）：
//   - 单一缓存池：`block_cache = buffer_pool_size`（默认 256 MiB），
//     `cache_index_and_filter_blocks = true`，即索引也占这份池——对应
//     EloqStore 的 `buffer_pool_size`（index + data 同池）和 fkv 的单页缓存；
//   - `block_size = 4 KiB`：与两侧 4 KiB 页同粒度，随机读都是 4 KiB；
//   - `compression = kNoCompression`：两侧都不压缩，体积行才可比；
//   - 持久化用 RocksDB **默认**：WAL 打开、`WriteOptions.sync = false`，即不逐次
//     fsync，落盘交给内核与 background flush（这正是 db_bench 默认口径）。
//     注意 EloqStore 每批固定 SyncData + AppendManifest（两次 fdatasync），
//     fkv 同样每批 1 次数据 + 1 次 manifest fdatasync，且都无法关闭——写阶段
//     两侧比 RocksDB 多付 fsync，这一条必须写进表下配置说明，不能当作纯实现差异。
//   - memtable（write_buffer_size）不属于页缓存，取 RocksDB 默认 64 MiB。

#include "bench_ffi.h"

#include <rocksdb/cache.h>
#include <rocksdb/db.h>
#include <rocksdb/iterator.h>
#include <rocksdb/options.h>
#include <rocksdb/table.h>
#include <rocksdb/write_batch.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <memory>
#include <numeric>
#include <random>
#include <string>
#include <thread>
#include <vector>

namespace
{

using Clock = std::chrono::steady_clock;
constexpr size_t kKeyIndexBytes = sizeof(uint64_t);
constexpr uint64_t kDefaultBlockCacheBytes = 256ull << 20;
constexpr size_t kBlockSize = 4096;

thread_local std::string tls_open_error;

double ElapsedSecs(Clock::time_point start)
{
    const double secs =
        std::chrono::duration<double>(Clock::now() - start).count();
    return secs > 0.0 ? secs : std::numeric_limits<double>::min();
}

double Percentile(std::vector<double> &samples, double p)
{
    if (samples.empty())
    {
        return 0.0;
    }
    const size_t rank = static_cast<size_t>(p * samples.size());
    const size_t idx = std::min(samples.size() - 1, rank);
    std::nth_element(samples.begin(), samples.begin() + idx, samples.end());
    return samples[idx];
}

}  // namespace

struct FkvBenchFfi
{
    std::string root;
    uint32_t rows{0};
    uint32_t bulk_rows{0};
    uint32_t batch_size{0};
    uint32_t kv_size{0};
    uint32_t key_size{0};
    uint32_t partitions{1};
    uint64_t cache_bytes{0};
    uint32_t individual_writes{0};
    uint32_t batch_writes{0};
    uint32_t removals{0};
    uint32_t reads{0};
    uint32_t range_scans{0};
    uint32_t range_len{0};
    std::unique_ptr<rocksdb::DB> db;
    bool loaded{false};
    /// 每批写是否 fsync（对齐 fkv / EloqStore 的持久化强度）。
    bool sync_writes{false};
    std::string last_error;
};

namespace
{

int Fail(FkvBenchFfi *h, std::string message)
{
    if (h != nullptr)
    {
        h->last_error = std::move(message);
    }
    else
    {
        tls_open_error = std::move(message);
    }
    return -1;
}

std::string KeyOf(const FkvBenchFfi *h, uint64_t value)
{
    std::string key(h->key_size, '\0');
    const size_t offset = key.size() - kKeyIndexBytes;
    for (int i = static_cast<int>(kKeyIndexBytes) - 1; i >= 0; --i)
    {
        key[offset + static_cast<size_t>(i)] =
            static_cast<char>(value & 0xff);
        value >>= 8;
    }
    return key;
}

std::string ValueOf(const FkvBenchFfi *h)
{
    return std::string(h->kv_size - h->key_size, 'v');
}

rocksdb::Options MakeOptions(const FkvBenchFfi *h)
{
    rocksdb::Options options;
    options.create_if_missing = true;
    options.compression = rocksdb::kNoCompression;
    // memtable 不属于页缓存，保持 RocksDB 默认；只为避免小默认值放大写放大。
    options.write_buffer_size = 64ull << 20;
    options.max_write_buffer_number = 2;
    options.target_file_size_base = 64ull << 20;

    rocksdb::BlockBasedTableOptions table;
    table.block_size = kBlockSize;
    table.cache_index_and_filter_blocks = true;
    const size_t cache_bytes = h->cache_bytes == 0
                                   ? kDefaultBlockCacheBytes
                                   : static_cast<size_t>(h->cache_bytes);
    table.block_cache = rocksdb::NewLRUCache(cache_bytes);
    options.table_factory.reset(rocksdb::NewBlockBasedTableFactory(table));
    return options;
}

/// 写选项：`sync_writes` 打开时每批 fsync，与 fkv / EloqStore 的持久化强度对齐；
/// 关闭时保持 RocksDB 默认（WAL 开、不逐批 fsync，db_bench 口径）。
rocksdb::WriteOptions WriteOptionsOf(FkvBenchFfi *h)
{
    rocksdb::WriteOptions wo;
    wo.sync = h->sync_writes;
    return wo;
}

/// 写一批 key（sync 与两侧持久化强度对齐）。
bool WriteBatchOf(FkvBenchFfi *h,
                  uint64_t first,
                  uint32_t count,
                  std::string *err)
{
    const std::string value = ValueOf(h);
    rocksdb::WriteBatch batch;
    for (uint32_t i = 0; i < count; ++i)
    {
        batch.Put(KeyOf(h, first + i), value);
    }
    // RocksDB 默认持久化口径：WAL 开启、不逐批 fsync。
    const rocksdb::Status st = h->db->Write(WriteOptionsOf(h), &batch);
    if (!st.ok())
    {
        *err = st.ToString();
        return false;
    }
    return true;
}

bool EnsureLoaded(FkvBenchFfi *h)
{
    if (h->loaded)
    {
        return true;
    }
    std::string err;
    for (uint64_t first = 0; first < h->rows; first += h->batch_size)
    {
        const uint32_t count = static_cast<uint32_t>(
            std::min<uint64_t>(h->batch_size, h->rows - first));
        if (!WriteBatchOf(h, static_cast<uint64_t>(first), count, &err))
        {
            Fail(h, "load: " + err);
            return false;
        }
    }
    h->loaded = true;
    return true;
}

int RunBulkLoad(FkvBenchFfi *h, FkvBenchResult *out)
{
    const auto start = Clock::now();
    std::string err;
    for (uint64_t first = 0; first < h->bulk_rows; first += h->batch_size)
    {
        const uint32_t count = static_cast<uint32_t>(
            std::min<uint64_t>(h->batch_size, h->bulk_rows - first));
        if (!WriteBatchOf(h, first, count, &err))
        {
            return Fail(h, "bulk_load: " + err);
        }
    }
    h->loaded = true;
    out->ops = h->bulk_rows;
    out->secs = ElapsedSecs(start);
    out->p99_us = 0.0;
    return 0;
}

int RunIndividualWrites(FkvBenchFfi *h, FkvBenchResult *out)
{
    if (!EnsureLoaded(h))
    {
        return -1;
    }
    const std::string value = ValueOf(h);
    std::vector<double> latencies;
    latencies.reserve(h->individual_writes);
    const auto start = Clock::now();
    for (uint32_t i = 0; i < h->individual_writes; ++i)
    {
        const auto t0 = Clock::now();
        // 单条写同样用对齐后的持久化强度。
        const rocksdb::Status st = h->db->Put(
            WriteOptionsOf(h), KeyOf(h, static_cast<uint64_t>(h->rows) + i), value);
        latencies.push_back(
            std::chrono::duration<double, std::micro>(Clock::now() - t0).count());
        if (!st.ok())
        {
            return Fail(h, "individual_writes: " + st.ToString());
        }
    }
    out->ops = h->individual_writes;
    out->secs = ElapsedSecs(start);
    out->p99_us = Percentile(latencies, 0.99);
    return 0;
}

int RunBatchWrites(FkvBenchFfi *h, FkvBenchResult *out)
{
    if (!EnsureLoaded(h))
    {
        return -1;
    }
    std::string err;
    const auto start = Clock::now();
    for (uint32_t b = 0; b < h->batch_writes; ++b)
    {
        const uint64_t first =
            static_cast<uint64_t>(h->rows) + static_cast<uint64_t>(b) * h->batch_size;
        if (!WriteBatchOf(h, first, h->batch_size, &err))
        {
            return Fail(h, "batch_writes: " + err);
        }
    }
    out->ops = static_cast<uint64_t>(h->batch_writes) * h->batch_size;
    out->secs = ElapsedSecs(start);
    out->p99_us = 0.0;
    return 0;
}

int RunRandomReads(FkvBenchFfi *h, uint32_t readers, FkvBenchResult *out)
{
    if (!EnsureLoaded(h))
    {
        return -1;
    }
    const uint32_t total_reads = h->reads == 0 ? h->rows : h->reads;
    const uint32_t thread_count = std::max<uint32_t>(1, std::min(readers, total_reads));
    const uint32_t base = total_reads / thread_count;
    const uint32_t remainder = total_reads % thread_count;

    std::atomic<uint64_t> completed{0};
    std::atomic<bool> failed{false};
    std::string first_error;
    std::vector<std::vector<double>> latencies(thread_count);
    std::vector<std::thread> threads;
    threads.reserve(thread_count);
    const auto start = Clock::now();
    for (uint32_t t = 0; t < thread_count; ++t)
    {
        const uint32_t count = base + (t < remainder ? 1 : 0);
        threads.emplace_back([h, t, count, &completed, &failed, &first_error,
                              &latencies]() {
            std::mt19937_64 rng(0x9e3779b97f4a7c15ULL * (t + 1));
            std::uniform_int_distribution<uint64_t> dist(0, h->rows - 1);
            std::string value;
            auto &lane = latencies[t];
            lane.reserve(count);
            for (uint32_t i = 0; i < count; ++i)
            {
                const std::string key = KeyOf(h, dist(rng));
                const auto t0 = Clock::now();
                const rocksdb::Status st = h->db->Get(rocksdb::ReadOptions(), key, &value);
                lane.push_back(std::chrono::duration<double, std::micro>(
                                   Clock::now() - t0)
                                   .count());
                if (!st.ok() && !st.IsNotFound())
                {
                    if (!failed.exchange(true))
                    {
                        first_error = st.ToString();
                    }
                    return;
                }
                completed.fetch_add(1, std::memory_order_relaxed);
            }
        });
    }
    for (auto &thread : threads)
    {
        thread.join();
    }
    if (failed.load())
    {
        return Fail(h, "random_reads: " + first_error);
    }
    std::vector<double> all;
    all.reserve(completed.load());
    for (auto &lane : latencies)
    {
        all.insert(all.end(), lane.begin(), lane.end());
    }
    out->ops = completed.load();
    out->secs = ElapsedSecs(start);
    out->p99_us = Percentile(all, 0.99);
    return 0;
}

int RunRangeReads(FkvBenchFfi *h, FkvBenchResult *out)
{
    if (!EnsureLoaded(h))
    {
        return -1;
    }
    const uint32_t scans = h->range_scans == 0 ? std::max<uint32_t>(1000, h->rows / 2)
                                               : h->range_scans;
    const uint32_t scan_len = h->range_len == 0 ? 10 : h->range_len;
    const uint64_t max_start =
        h->rows > scan_len ? static_cast<uint64_t>(h->rows) - scan_len : 1;
    std::mt19937_64 rng(0xd1b54a32d192ed03ULL);
    std::uniform_int_distribution<uint64_t> dist(0, max_start - 1);
    std::vector<double> latencies;
    latencies.reserve(scans);
    const auto start = Clock::now();
    for (uint32_t i = 0; i < scans; ++i)
    {
        const std::string begin = KeyOf(h, dist(rng));
        rocksdb::ReadOptions ro;
        std::unique_ptr<rocksdb::Iterator> iter(h->db->NewIterator(ro));
        const auto t0 = Clock::now();
        iter->Seek(begin);
        for (uint32_t n = 0; n < scan_len && iter->Valid(); ++n)
        {
            iter->Next();
        }
        latencies.push_back(
            std::chrono::duration<double, std::micro>(Clock::now() - t0).count());
        if (!iter->status().ok())
        {
            return Fail(h, "range_reads: " + iter->status().ToString());
        }
    }
    out->ops = scans;
    out->secs = ElapsedSecs(start);
    out->p99_us = Percentile(latencies, 0.99);
    return 0;
}

int RunRemovals(FkvBenchFfi *h, FkvBenchResult *out)
{
    if (!EnsureLoaded(h))
    {
        return -1;
    }
    const auto start = Clock::now();
    for (uint64_t offset = 0; offset < h->removals; offset += h->batch_size)
    {
        const uint32_t count = static_cast<uint32_t>(
            std::min<uint64_t>(h->batch_size, h->removals - offset));
        rocksdb::WriteBatch batch;
        for (uint32_t i = 0; i < count; ++i)
        {
            batch.Delete(KeyOf(h, offset + i));
        }
        const rocksdb::Status st = h->db->Write(WriteOptionsOf(h), &batch);
        if (!st.ok())
        {
            return Fail(h, "removals: " + st.ToString());
        }
    }
    out->ops = h->removals;
    out->secs = ElapsedSecs(start);
    out->p99_us = 0.0;
    return 0;
}

int RunLen(FkvBenchFfi *h, FkvBenchResult *out)
{
    if (!EnsureLoaded(h))
    {
        return -1;
    }
    uint64_t count = 0;
    const auto start = Clock::now();
    std::unique_ptr<rocksdb::Iterator> iter(h->db->NewIterator(rocksdb::ReadOptions()));
    for (iter->SeekToFirst(); iter->Valid(); iter->Next())
    {
        ++count;
    }
    if (!iter->status().ok())
    {
        return Fail(h, "len: " + iter->status().ToString());
    }
    out->ops = count;
    out->secs = ElapsedSecs(start);
    out->p99_us = 0.0;
    return 0;
}

int RunCompact(FkvBenchFfi *h, FkvBenchResult *out)
{
    const auto start = Clock::now();
    h->db->CompactRange(rocksdb::CompactRangeOptions(), nullptr, nullptr);
    out->ops = 0;
    out->secs = ElapsedSecs(start);
    out->p99_us = 0.0;
    return 0;
}

}  // namespace

extern "C"
{

uint32_t fkv_bench_abi_version(void)
{
    return FKV_BENCH_ABI_VERSION;
}

FkvBenchFfi *fkv_bench_open(const char *root, const FkvBenchConfig *config)
{
    if (root == nullptr || config == nullptr)
    {
        tls_open_error = "fkv_bench_open: null root or config";
        return nullptr;
    }
    auto handle = std::make_unique<FkvBenchFfi>();
    handle->root = root;
    // 评测口径开关：FKV_BENCH_SYNC=1 让 RocksDB 也逐批 fsync，与 fkv /
    // EloqStore 的持久化强度一致（两者都没有关闭同步的开关）。
    if (const char *sync_env = std::getenv("FKV_BENCH_SYNC"))
    {
        handle->sync_writes = (std::string(sync_env) == "1");
    }
    handle->rows = config->rows;
    handle->bulk_rows = config->bulk_rows;
    handle->batch_size = std::max<uint32_t>(1, config->batch_size);
    handle->kv_size = config->kv_size;
    handle->key_size = config->key_size;
    handle->partitions = std::max<uint32_t>(1, config->partitions);
    handle->cache_bytes = config->buffer_pool_size;
    handle->individual_writes = config->individual_writes;
    handle->batch_writes = config->batch_writes;
    handle->removals = config->removals;
    handle->reads = config->reads;
    handle->range_scans = config->range_scans;
    handle->range_len = config->range_len;

    std::error_code ec;
    std::filesystem::create_directories(handle->root, ec);
    rocksdb::Options options = MakeOptions(handle.get());
    std::unique_ptr<rocksdb::DB> db;
    const rocksdb::Status st =
        rocksdb::DB::Open(options, handle->root, &db);
    if (!st.ok())
    {
        tls_open_error = "rocksdb open: " + st.ToString();
        return nullptr;
    }
    handle->db = std::move(db);
    return handle.release();
}

void fkv_bench_close(FkvBenchFfi *h)
{
    delete h;
}

int fkv_bench_run(FkvBenchFfi *h,
                  const char *phase,
                  uint32_t readers,
                  double /*read_secs*/,
                  FkvBenchResult *out)
{
    if (h == nullptr || phase == nullptr || out == nullptr)
    {
        return Fail(h, "fkv_bench_run: null handle/phase/out");
    }
    h->last_error.clear();
    *out = FkvBenchResult{0, 0.0, 0.0};
    const std::string name(phase);
    if (name == "bulk_load")
    {
        return RunBulkLoad(h, out);
    }
    if (name == "individual_writes")
    {
        return RunIndividualWrites(h, out);
    }
    if (name == "batch_writes")
    {
        return RunBatchWrites(h, out);
    }
    if (name == "random_reads")
    {
        return RunRandomReads(h, readers, out);
    }
    if (name == "range_reads")
    {
        return RunRangeReads(h, out);
    }
    if (name == "removals")
    {
        return RunRemovals(h, out);
    }
    if (name == "len")
    {
        return RunLen(h, out);
    }
    if (name == "compact")
    {
        return RunCompact(h, out);
    }
    return Fail(h, "fkv_bench_run: unknown phase: " + name);
}

const char *fkv_bench_last_error(FkvBenchFfi *h)
{
    if (h != nullptr)
    {
        return h->last_error.c_str();
    }
    return tls_open_error.c_str();
}

}  // extern "C"
