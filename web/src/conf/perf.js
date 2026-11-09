// 首页性能矩阵数据：Rust fkv vs RocksDB vs fjall，同一阶段形状、同一缓存额度。
// 数据源：CI 运行 42553224769（fkv-compare.json）。
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
    ["来源", "CI 运行 42553224769"],
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
  ["批量加载", "1027300", "1364447", "730797", "0.753x", "1.406x"],
  ["单条写入", "1133", "419503", "763070", "0.00270x", "0.00149x", "bad"],
  ["批量写", "979303", "2862401", "2465481", "0.342x", "0.397x", "bad"],
  ["len()", "30414449", "4389444", "5197385", "6.929x", "5.852x"],
  ["随机读（1 线程）", "268998", "206230", "257767", "1.304x", "1.044x"],
  ["随机读（4 线程）", "1003809", "609854", "259477", "1.646x", "3.869x"],
  ["随机读（8 线程）", "1012193", "617827", "262135", "1.638x", "3.861x"],
  ["随机读（16 线程）", "1027890", "633687", "263243", "1.622x", "3.905x"],
  ["随机读（32 线程）", "1018907", "634040", "262915", "1.607x", "3.875x"],
  ["随机范围读", "223880", "123340", "157827", "1.815x", "1.419x"],
  ["删除", "1207405", "2011685", "2511019", "0.600x", "0.481x"],
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
