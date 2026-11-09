// 首页性能矩阵数据：Rust fkv vs RocksDB vs fjall，同一阶段形状、同一缓存额度。
// 数据源：CI 运行 42566872585（fkv-compare.json）。
// 空值在页面上显示为 —。手改数字会使它与产物对不上，请走脚本。
export const PERF_META =
  "Rust fkv vs RocksDB vs fjall · 2026-11-09 · GitHub Actions ubuntu-latest (x86_64, 4 KiB 页) · 25,000,000 行 × 116 B ≈ 2766 MiB 数据 / 256 MiB 缓存（≈11:1，对齐 RocksDB 官方 97 GiB / 6 GiB）· readers 1/4/8/16/32 · 每阶段 1 次采样";

// [阶段, fkv, RocksDB, fjall, fkv/RocksDB, fkv/fjall, tone]
// tone: "bad" 表示两个对照都比 fkv 快一倍以上（ratio < 0.5）；体积行不判 tone。
// 表下配置分块：[标题, [[项, 值], ...]]，各自成卡，不挤成一行。
export const PERF_CONFIG = {
  "机器": [
    ["机器", "GitHub Actions ubuntu-latest (x86_64, 4 KiB 页)"],
    ["页大小", "4 KiB（fkv data page = RocksDB block_size）"],
    ["日期", "2026-11-09"],
    ["来源", "CI 运行 42566872585"],
    ["口径", "三个引擎同一台机器、同一轮次、同一份数据形状"],
    ["并发口径", "readers 档位三方都是真并发：fkv 每读者一个 OS 线程 + 独立 Store（页缓存平分），RocksDB / fjall 等量 std::thread；fkv 在 16/32 档位受 4 vCPU 超订与缓存平分影响，见 doc/perf-compare.md"],
  ],
  "测试负载": [
    ["数据量", "25,000,000 行 × 116 B ≈ 2766 MiB"],
    ["缓存/引擎", "256 MiB（数据 ≈ 10.8× 缓存）"],
    ["key / value", "16 B / 100 B（db_bench 默认）"],
    ["点读", "1,000,000 次（并发 1/4/8/16/32）"],
    ["范围读", "500,000 次 × 10 条"],
    ["删除", "500,000 条"],
    ["批量", "每批 1000 条"],
  ],
  "持久化 / fsync": [
    ["fkv", "每批 1×数据页 + 1×manifest fdatasync（共 2 次，无关闭开关）"],
    ["RocksDB", "默认配置：WAL 开、WriteOptions.sync=false（不逐次 fsync）"],
    ["fjall", "默认配置：PersistMode::Buffer（不逐次 fsync）"],
    ["影响", "写阶段的差距来自这里：fkv 每批付两次同步，另外两列用默认配置不付；读阶段不受影响"],
  ],
  "RocksDB 列": [
    ["版本", "系统包 librocksdb（dlopen 同一 C ABI）"],
    ["block cache", "256 MiB LRU（与两侧缓存同额）"],
    ["block size", "4 KiB"],
    ["index/filter", "cache_index_and_filter_blocks=true（同池）"],
    ["压缩", "kNoCompression（三侧都不压缩）"],
    ["memtable", "write_buffer_size 64 MiB（不属于页缓存）"],
    ["注意", "RocksDB 用默认持久化配置（不逐次 fsync），不为了对齐修改它的默认值；memtable 不属于页缓存"],
  ],
};

export const PERF_MATRIX_ROWS = [
  ["批量加载", "975186", "1266615", "726907", "0.770x", "1.342x"],
  ["单条写入", "1430", "335966", "677126", "0.00426x", "0.00211x", "bad"],
  ["批量写", "1088793", "2192776", "2795055", "0.497x", "0.390x", "bad"],
  ["len()", "22758969", "3499066", "4409824", "6.504x", "5.161x"],
  ["随机读（1 线程）", "213293", "160897", "198094", "1.326x", "1.077x"],
  ["随机读（4 线程）", "768896", "446099", "187465", "1.724x", "4.102x"],
  ["随机读（8 线程）", "751843", "451040", "188827", "1.667x", "3.982x"],
  ["随机读（16 线程）", "752526", "452102", "189492", "1.665x", "3.971x"],
  ["随机读（32 线程）", "739436", "464977", "193090", "1.590x", "3.829x"],
  ["随机范围读", "164616", "91339", "117742", "1.802x", "1.398x"],
  ["删除", "1094718", "1455183", "2367939", "0.752x", "0.462x"],
  ["盘面体积", "2.79 GiB", "2.67 GiB", "2.68 GiB", null, null]
];

export const PERF_NOTE =
  "单位：吞吐行为操作数/秒（ops/s），体积行为 MiB。`fkv / RocksDB`、`fkv / fjall` 大于 1 表示 fkv 更快（蓝色），小于 0.5 表示慢一倍以上（红色）。三个引擎同一条 key 编码、同一批阶段、同为 256 MiB 缓存；对照列用各自的默认配置，不修改 RocksDB 的默认持久化（`WriteOptions.sync=false`）。fkv 每批固定两次 fdatasync（数据页 + manifest，与 C++ EloqStore 同构且无法关闭），所以「单条写入 / 批量写」的差距是持久化强度差异，不是实现差异：同强度口径（`--align-fsync`）下这两行 fkv 反而领先。读、len()、范围读、批量加载、删除不受影响。";

export const VERIFY_ROWS = [
  ["tokio 依赖", "全仓 0 处"],
  ["S3 客户端", "fkv_s3（SigV4 对拍 AWS 向量）"],
  ["矩阵数字", "可追溯到 CI artifact"],
  ["工作区检查", "cargo check --all-targets 干净"]
];

export const DOC_LINKS = [
  ["README", "项目说明"],
  ["architecture", "架构总览"],
  ["performance", "性能口径"],
  ["crates", "crate 布局"],
  ["testing", "测试"]
];
