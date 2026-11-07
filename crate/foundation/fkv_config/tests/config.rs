use std::{
  fs::write,
  mem::take,
  sync::{Mutex, Once},
  time::Duration,
};

use fkv_config::{
  DEFAULT_CLOUD_QUEUE_TIMEOUT_US, DEFAULT_CLOUD_REQUEST_TIMEOUT_US, IO_QUEUE_SIZE_WARN_BELOW,
  KvOption, MIN_IO_QUEUE_SIZE,
};
use fkv_error::KvError;
use fkv_testkit::TempRoot;
use fkv_types::StoreMode;
use log::{Level, LevelFilter};

/// 捕获 `validate` 的 warn 日志；与 `fkv_store/tests/clean_expired_bound.rs`
/// 同一模式。nextest 每测试独立进程，不存在跨测试串扰。
static LOG_LI: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LOG_INIT: Once = Once::new();

struct CaptureLogger;

impl log::Log for CaptureLogger {
  fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
    metadata.level() <= Level::Warn
  }

  fn log(&self, record: &log::Record<'_>) {
    if self.enabled(record.metadata()) {
      LOG_LI.lock().unwrap().push(record.args().to_string());
    }
  }

  fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger;

fn init_log() {
  LOG_INIT.call_once(|| {
    log::set_logger(&LOGGER).expect("测试日志器只注册一次");
    log::set_max_level(LevelFilter::Warn);
  });
}

fn take_log_li() -> Vec<String> {
  take(&mut *LOG_LI.lock().unwrap())
}

#[test]
fn default_is_local() {
  let opts = KvOption::default();
  assert_eq!(opts.derive_mode(), StoreMode::Local);
  assert_eq!(opts.data_page_size, 4096);
  assert_eq!(opts.pages_per_file_shift, 18);
}

/// io-40：两个写 IO 上限的默认值对齐 C++ `kv_options.h` 字段初始值
/// （`32 << 10` / `32`），INI 可覆盖且互不影响。
#[test]
fn write_limits_default_and_ini_override() {
  let opts = KvOption::default();
  assert_eq!(opts.max_inflight_write, 32 << 10);
  assert_eq!(opts.max_write_batch_pages, 32);

  let root = TempRoot::new("fkv_write_limits");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nnum_threads = 1\nmax_inflight_write = 17\nmax_write_batch_pages = 3\n[permanent]\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.max_inflight_write, 17);
  assert_eq!(opts.max_write_batch_pages, 3);
}

#[test]
fn cloud_mode_clears_standby() {
  let mut opts = KvOption {
    cloud_store_path: "bucket/prefix/".to_owned(),
    enable_local_standby: true,
    standby_master_addr: "host".to_owned(),
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.cloud_store_path, "bucket/prefix");
  assert_eq!(opts.derive_mode(), StoreMode::Cloud);
  assert!(!opts.enable_local_standby);
}

#[test]
fn standby_modes() {
  let mut opts = KvOption {
    enable_local_standby: true,
    store_path: vec!["/tmp/fkv".to_owned()],
    ..KvOption::default()
  };
  assert_eq!(opts.derive_mode(), StoreMode::StandbyMaster);
  opts.standby_master_addr = "host".to_owned();
  assert_eq!(opts.derive_mode(), StoreMode::StandbyReplica);
}

/// derive_mode 优先级对齐 C++ `DeriveStoreMode`：standby 先于 cloud。
/// `validate` 后再 derive 仍得 Cloud（validate 清 standby 字段），两条路径都与 C++ 一致。
#[test]
fn derive_mode_priority_matrix() {
  let case_li = [
    (false, "", "", StoreMode::Local),
    (false, "", "bucket/prefix", StoreMode::Cloud),
    (true, "", "", StoreMode::StandbyMaster),
    (true, "host", "", StoreMode::StandbyReplica),
    // 双开关同时置位且未 validate：standby 优先。
    (true, "", "bucket/prefix", StoreMode::StandbyMaster),
    (true, "host", "bucket/prefix", StoreMode::StandbyReplica),
  ];
  for (enable, addr, cloud, want) in case_li {
    let opts = KvOption {
      enable_local_standby: enable,
      standby_master_addr: addr.to_owned(),
      cloud_store_path: cloud.to_owned(),
      ..KvOption::default()
    };
    assert_eq!(
      opts.derive_mode(),
      want,
      "enable_local_standby={enable} addr={addr:?} cloud={cloud:?}"
    );
  }

  // validate 清 standby 字段：双开关场景收敛到 Cloud。
  let mut opts = KvOption {
    enable_local_standby: true,
    standby_master_addr: "host".to_owned(),
    cloud_store_path: "bucket/prefix".to_owned(),
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.derive_mode(), StoreMode::Cloud);
}

#[test]
fn invalid_options() {
  let mut opts = KvOption {
    num_threads: 0,
    ..KvOption::default()
  };
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);

  // restart 间隔 0 会在页构建器里失败，提前到配置层拒绝。
  for zero in [
    KvOption {
      data_page_restart_interval: 0,
      ..KvOption::default()
    },
    KvOption {
      index_page_restart_interval: 0,
      ..KvOption::default()
    },
    KvOption {
      max_inflight_write: 0,
      ..KvOption::default()
    },
    KvOption {
      max_write_batch_pages: 0,
      ..KvOption::default()
    },
  ] {
    let mut opts = zero;
    assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);
  }

  let mut opts = KvOption {
    standby_master_addr: "host".to_owned(),
    ..KvOption::default()
  };
  assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);
}

/// io-10：compio 0.19.2 的 SQ 满后不重排唤醒，过小的 `io_queue_size` 会把
/// 并发突发放大成永久 poll 阻塞。硬下限拒绝，告警阈值只提示（保留 C++ 侧
/// 已有的小 ring 调优空间）。
#[test]
fn io_queue_size_guard() {
  init_log();

  let mut opts = KvOption::default();
  take_log_li();
  opts.validate().unwrap();
  assert!(
    !take_log_li()
      .iter()
      .any(|line| line.contains("io_queue_size")),
    "默认 4096 不应告警"
  );

  for value in [0, MIN_IO_QUEUE_SIZE - 1] {
    let mut opts = KvOption {
      io_queue_size: value,
      ..KvOption::default()
    };
    assert_eq!(
      opts.validate().unwrap_err(),
      KvError::InvalidArgs,
      "io_queue_size={value} 应被拒绝"
    );
  }

  // 硬下限通过，但仍低于告警阈值：放行 + warn。
  let mut opts = KvOption {
    io_queue_size: MIN_IO_QUEUE_SIZE,
    ..KvOption::default()
  };
  take_log_li();
  opts.validate().unwrap();
  let log_li = take_log_li();
  assert!(
    log_li
      .iter()
      .any(|line| line.contains(&format!("io_queue_size={MIN_IO_QUEUE_SIZE}"))),
    "低于 {IO_QUEUE_SIZE_WARN_BELOW} 应告警：{log_li:?}"
  );

  // 告警阈值边界：等于阈值不告警。
  let mut opts = KvOption {
    io_queue_size: IO_QUEUE_SIZE_WARN_BELOW,
    ..KvOption::default()
  };
  take_log_li();
  opts.validate().unwrap();
  assert!(
    !take_log_li()
      .iter()
      .any(|line| line.contains("io_queue_size")),
    "等于告警阈值不应告警"
  );
}

/// fmt-18：data / index 两个 restart 旋钮独立解析，互不影响。
#[test]
fn ini_restart_intervals_are_independent() {
  let root = TempRoot::new("fkv_restart");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nnum_threads = 1\ndata_page_restart_interval = 8\n[permanent]\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.data_page_restart_interval, 8);
  assert_eq!(opts.index_page_restart_interval, 16);

  write(
    &path,
    "[run]\nnum_threads = 1\nindex_page_restart_interval = 4\n[permanent]\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.data_page_restart_interval, 16);
  assert_eq!(opts.index_page_restart_interval, 4);
}

#[test]
fn load_ini() {
  let root = TempRoot::new("fkv_options");
  let path = root.join("config.ini");
  write(
        &path,
        "[run]\nnum_threads = 4\nbuffer_pool_size = 400MB\n[permanent]\nstore_path = /tmp/a, /tmp/b\ndata_page_size = 4KB\ndata_file_size = 8MB\ndata_append_mode = true\n",
    )
    .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 4);
  assert_eq!(opts.buffer_pool_size, 400 * 1024 * 1024);
  assert!(opts.data_append_mode);
  assert_eq!(
    opts.store_path,
    vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()]
  );
  assert!(opts.store_path_weights.is_empty());
  assert_eq!(opts.data_page_size, 4096);
  assert_eq!(opts.pages_per_file_shift, 11);
}

/// 键只在所属节内生效；[run] 里的 [permanent] 键被忽略。
#[test]
fn ini_section_scope() {
  let root = TempRoot::new("fkv_section");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nnum_threads = 4\nstore_path = /tmp/wrong\ndata_append_mode = true\n[permanent]\ndata_page_size = 4KB\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 4);
  // store_path 属 [permanent]，写在 [run] 不生效。
  assert!(opts.store_path.is_empty());
  // data_append_mode 属 [permanent]，写在 [run] 不生效。
  assert!(!opts.data_append_mode);
}

/// 缺 [run] 或 [permanent] 节报错（对齐 C++ 返回 -2）。
#[test]
fn ini_missing_section_rejected() {
  let root = TempRoot::new("fkv_missing");
  let path = root.join("config.ini");
  for text in [
    "[run]\nnum_threads = 4\n",
    "[permanent]\nstore_path = /tmp/a\n",
  ] {
    write(&path, text).unwrap();
    let mut opts = KvOption::default();
    assert_eq!(opts.load_from_ini(&path).unwrap_err(), KvError::InvalidArgs);
  }
}

/// sem-19：节存在性按节头判定（有意宽松，见模块文档「INI 节存在性」）。
/// C++ `HasSection` 要求节内至少一条 `name=value`（空节返回 -2），Rust 把
/// 空节视为存在并跑默认值；本用例锁定宽松口径，防止后续改动无声收紧。
#[test]
fn ini_empty_section_is_accepted_as_present() {
  let root = TempRoot::new("fkv_ini_empty_section");
  let path = root.join("config.ini");
  // 空 [run]（只有注释）+ 有键 [permanent]：成功，[run] 全默认。
  write(
    &path,
    "[run]\n# 只有注释，没有任何键\n[permanent]\nstore_path = /tmp/fkv\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.store_path, vec!["/tmp/fkv".to_owned()]);
  assert_eq!(opts.num_threads, KvOption::default().num_threads);

  // 空 [permanent] + 有键 [run]：同样成功。
  write(&path, "[run]\nnum_threads = 3\n[permanent]\n").unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 3);
}

/// sem-19：inih 不 trim 节名（`[ run ]` 的节名是 `" run "`，C++
/// `HasSection("run")` 不命中），Rust 对节名做 trim 后匹配：`[ run ]` /
/// `[ permanent ]` 与 `[run]` / `[permanent]` 等价。锁定 Rust 宽松口径。
#[test]
fn ini_section_name_whitespace_is_trimmed() {
  let root = TempRoot::new("fkv_ini_section_space");
  let path = root.join("config.ini");
  write(
    &path,
    "[ run ]\nnum_threads = 5\n[ permanent ]\nstore_path = /tmp/space\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 5);
  assert_eq!(opts.store_path, vec!["/tmp/space".to_owned()]);
}

/// sem-06：节名与键名按 ASCII 小写匹配（对齐 inih `INIReader::MakeKey`），
/// `[Run]` / `Num_Threads` 均命中。
#[test]
fn ini_section_and_key_names_are_case_insensitive() {
  let root = TempRoot::new("fkv_ini_case");
  let path = root.join("config.ini");
  write(
    &path,
    "[Run]\nNum_Threads = 7\n[PERMANENT]\nStore_Path = /tmp/case\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 7);
  assert_eq!(opts.store_path, vec!["/tmp/case".to_owned()]);
}

/// sem-06：文件头 UTF-8 BOM 剥离（对齐 inih `INI_ALLOW_BOM=1`）。
#[test]
fn ini_bom_prefix_is_stripped() {
  let root = TempRoot::new("fkv_ini_bom");
  let path = root.join("config.ini");
  write(&path, "\u{feff}[run]\nnum_threads = 3\n[permanent]\n").unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 3);
}

/// sem-06：值内 `;` 行内注释对齐 inih——只切前有空白字符的 `;`；
/// `#` 不是行内注释符（C++ 只在行首把 `#` 当注释）。
#[test]
fn ini_value_inline_comment_is_stripped() {
  let root = TempRoot::new("fkv_ini_comment");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nnum_threads = 8 ; eight\n[permanent]\ncloud_endpoint = http://host:9000 ; note\ncloud_region = us;east\ncloud_provider = aws # keep\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 8);
  assert_eq!(opts.cloud_endpoint, "http://host:9000");
  // inih 要求注释符前有空白：`;` 紧跟字符时按值内容保留。
  assert_eq!(opts.cloud_region, "us;east");
  assert_eq!(opts.cloud_provider, "aws # keep");
}

/// sem-06：`:` 与 `=` 同为键值分隔符，取行内最先出现者（对齐 inih
/// `ini_find_chars_or_comment(start, "=:")`）；值内的 `:` 不误切。
#[test]
fn ini_colon_separator_is_accepted() {
  let root = TempRoot::new("fkv_ini_colon");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nnum_threads: 6\n[permanent]\ncloud_endpoint: http://host:9000/path\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.num_threads, 6);
  assert_eq!(opts.cloud_endpoint, "http://host:9000/path");
}

/// sem-06：数值键保持严格解析（有意从严），不模拟 C++ `strtoul(base=0)`
/// 的前缀 / 回绕：`8abc`、`0x10`、`-1` 均报 `InvalidArgs`。
#[test]
fn ini_numeric_values_are_strict() {
  let root = TempRoot::new("fkv_ini_strict");
  let path = root.join("config.ini");
  for bad in ["8abc", "0x10", "-1"] {
    write(&path, format!("[run]\nnum_threads = {bad}\n[permanent]\n")).unwrap();
    let mut opts = KvOption::default();
    assert_eq!(
      opts.load_from_ini(&path).unwrap_err(),
      KvError::InvalidArgs,
      "num_threads = {bad} 应被拒绝"
    );
  }
}

/// sem-06：Rust 不设行长上限（C++ `INI_MAX_LINE=200` 会截断超长行），
/// >200 字符的 `store_path` 必须完整解析。
#[test]
fn ini_long_value_is_not_truncated() {
  let root = TempRoot::new("fkv_ini_long");
  let path = root.join("config.ini");
  let long_path = format!("/{}", "a".repeat(300));
  assert!(long_path.len() > 200);
  write(
    &path,
    format!("[run]\nnum_threads = 1\n[permanent]\nstore_path = {long_path}\n"),
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.store_path, vec![long_path]);
}

/// sem-13：布尔合法词表与 C++ `GetBoolean` 一致（`true/yes/on/1`、
/// `false/no/off/0`，大小写不敏感）；词表外值 Rust 报 `InvalidArgs`，不学
/// C++ 静默回退调用点默认值（与数值键同一有意从严口径）。
#[test]
fn ini_bool_vocabulary_is_case_insensitive() {
  let root = TempRoot::new("fkv_ini_bool");
  let path = root.join("config.ini");
  let cases = [
    ("true", true),
    ("TRUE", true),
    ("Yes", true),
    ("on", true),
    ("1", true),
    ("false", false),
    ("FALSE", false),
    ("No", false),
    ("off", false),
    ("0", false),
  ];
  for (word, expected) in cases {
    write(
      &path,
      format!("[run]\nskip_verify_checksum = {word}\n[permanent]\n"),
    )
    .unwrap();
    // 预置反向初值：确保断言看到的是 INI 赋值，而不是默认值。
    let mut opts = KvOption {
      skip_verify_checksum: !expected,
      ..KvOption::default()
    };
    opts.load_from_ini(&path).unwrap();
    assert_eq!(
      opts.skip_verify_checksum, expected,
      "skip_verify_checksum = {word} 应解析为 {expected}"
    );
  }
}

/// sem-13：词表外布尔值（`maybe` / `2` / 空值）保持严格报错。C++ 对同样
/// 输入静默回落调用点默认值（`INIReader.cpp` 的 `else return default_value;`），
/// 差分口径见 `fkv_config` 模块文档与 `COMPAT_BASELINE`「INI 解析边界」。
#[test]
fn ini_bool_invalid_value_is_rejected() {
  let root = TempRoot::new("fkv_ini_bool_bad");
  let path = root.join("config.ini");
  for bad in ["maybe", "2", ""] {
    write(
      &path,
      format!("[run]\nskip_verify_checksum = {bad}\n[permanent]\n"),
    )
    .unwrap();
    let mut opts = KvOption::default();
    assert_eq!(
      opts.load_from_ini(&path).unwrap_err(),
      KvError::InvalidArgs,
      "skip_verify_checksum = {bad:?} 应被拒绝"
    );
  }
}

/// sem-13：`enable_compression` / `zstd_compression_level` 是 fkv 扩展键
/// （C++ `LoadFromIni` 不解析，见模块文档「INI 扩展键」），`[run]` 与
/// `[permanent]` 均生效。
#[test]
fn ini_compression_keys_are_rust_extension() {
  let root = TempRoot::new("fkv_ini_compress");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nenable_compression = true\nzstd_compression_level = 7\n[permanent]\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert!(opts.enable_compression);
  assert_eq!(opts.zstd_compression_level, 7);

  write(
    &path,
    "[run]\n[permanent]\nenable_compression = off\nzstd_compression_level = 1\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert!(!opts.enable_compression);
  assert_eq!(opts.zstd_compression_level, 1);
}

/// store_path 支持 `path1,path2:w1,w2` 内联权重（对齐 C++ `ParseStorePathListWithWeights`）。
#[test]
fn ini_store_path_inline_weights() {
  let root = TempRoot::new("fkv_weights");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nnum_threads = 1\n[permanent]\nstore_path = /tmp/a, /tmp/b:3, 7\nstandby_master_store_paths = /m1,/m2:1,1\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(
    opts.store_path,
    vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()]
  );
  assert_eq!(opts.store_path_weights, vec![3, 7]);
  assert_eq!(opts.standby_master_store_path_weights, vec![1, 1]);

  // 权重个数与路径不符 / 权重非法：报错（对齐 C++ 返回 -3）。
  for bad in ["store_path = /tmp/a,/tmp/b:3", "store_path = /tmp/a:x"] {
    write(
      &path,
      format!("[run]\nnum_threads = 1\n[permanent]\n{bad}\n"),
    )
    .unwrap();
    let mut opts = KvOption::default();
    assert_eq!(opts.load_from_ini(&path).unwrap_err(), KvError::InvalidArgs);
  }
}

/// data_file_size 非二次幂倍向下取整，小于 page size 回退单页（不报错）。
#[test]
fn ini_data_file_size_rounding() {
  let root = TempRoot::new("fkv_filesize");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\ndata_file_size = 12MB\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  // 12MB / 4KB = 3072 页 → floor(log2) = 11 → 8MB
  assert_eq!(opts.pages_per_file_shift, 11);

  write(
    &path,
    "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\ndata_file_size = 1KB\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.pages_per_file_shift, 0);
}

/// `data_file_size` 超过 `u32::MAX` 时保持 u64 语义，不模拟 C++ 的 u32 截断。
///
/// C++ `LoadFromIni`（`src/kv_options.cpp:346-361`）把 `ParseSizeWithUnit`
/// 返回的 `uint64_t` 赋给局部 `uint32_t` 再推导 shift：5GB 截成 1GiB（shift 18）、
/// 4GB / 8GB 截成 0（回退单页，shift 0）。Rust 全程 u64，差异登记在
/// `COMPAT_BASELINE`「范围与排除项」，本测试按 u64 语义钉住边界。
#[test]
fn data_file_size_above_u32_is_not_truncated() {
  let root = TempRoot::new("fkv_filesize_u64");
  let path = root.join("config.ini");
  const GB: u64 = 1 << 30;
  // 4 KiB 页：5GB = 5 * 2^30 / 2^12 = 1_310_720 页 → floor(log2) = 20；
  // 4GB = 2^32 / 2^12 = 2^20 页 → shift 20；
  // 8GB = 2^33 / 2^12 = 2^21 页 → shift 21。
  // `data_page_size << shift` 分别是 4GiB / 4GiB / 8GiB（5GB 非二次幂倍，向下取整）。
  let case_li = [
    ("5GB", 20, 4 * GB),
    ("4GB", 20, 4 * GB),
    ("8GB", 21, 8 * GB),
  ];
  for (size_text, shift, effective_size) in case_li {
    write(
      &path,
      format!(
        "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\ndata_file_size = {size_text}\n"
      ),
    )
    .unwrap();
    let mut opts = KvOption::default();
    opts.load_from_ini(&path).unwrap();
    opts.validate().unwrap();
    assert_eq!(
      opts.pages_per_file_shift, shift,
      "data_file_size = {size_text}"
    );
    assert_eq!(
      u64::from(opts.data_page_size) << opts.pages_per_file_shift,
      effective_size,
      "data_file_size = {size_text}"
    );
  }
}

/// 单位文法对齐 C++ `ParseSizeWithUnit`：只认两位尾缀 KB/MB/GB/TB，无单位按字节。
#[test]
fn ini_size_units_match_cpp() {
  let root = TempRoot::new("fkv_units");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nbuffer_pool_size = 400MB\nroot_meta_cache_size = 1gb\nmanifest_limit = 128\n[permanent]\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.buffer_pool_size, 400 * 1024 * 1024);
  assert_eq!(opts.root_meta_cache_size, 1024 * 1024 * 1024);
  assert_eq!(opts.manifest_limit, 128);

  // C++ 对单字母单位（400M）解析失败返回 0；fkv 直接拒绝，避免静默降级。
  for bad in [
    "buffer_pool_size = 400M",
    "buffer_pool_size = 400 B",
    "buffer_pool_size = 400XB",
  ] {
    write(&path, format!("[run]\n{bad}\n[permanent]\n")).unwrap();
    let mut opts = KvOption::default();
    assert_eq!(opts.load_from_ini(&path).unwrap_err(), KvError::InvalidArgs);
  }
}

/// `validate` 补齐 C++ `ValidateOption` 的 cloud 派生与 WARNING。
#[test]
fn validate_cloud_rewrites() {
  let mut opts = KvOption {
    cloud_store_path: "bucket/prefix".to_owned(),
    cloud_provider: "GCS".to_owned(),
    max_write_concurrency: 0,
    local_space_limit: 0,
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.cloud_endpoint, "https://storage.googleapis.com");
  assert_eq!(opts.max_write_concurrency, opts.max_cloud_concurrency);
  assert_eq!(opts.local_space_limit, fkv_config::TB);
  assert!(opts.data_append_mode);

  // 显式 endpoint 不被覆盖。
  let mut opts = KvOption {
    cloud_store_path: "bucket/prefix".to_owned(),
    cloud_provider: "google-cloud".to_owned(),
    cloud_endpoint: "http://minio:9000".to_owned(),
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.cloud_endpoint, "http://minio:9000");
}

/// standby 模式自动开启追加写（对齐 C++ WARNING 后 rewrite）。
#[test]
fn validate_standby_enables_append() {
  let mut opts = KvOption {
    enable_local_standby: true,
    store_path: vec!["/tmp/fkv".to_owned()],
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert!(opts.data_append_mode);
}

/// `data_append_mode` 在 local 模式是语义登记：false 不报错、不被翻转，
/// 引擎恒 append（pooled 不在 L1 范围）。
#[test]
fn validate_local_append_mode_inert() {
  let mut opts = KvOption::default();
  assert!(!opts.data_append_mode);
  opts.validate().unwrap();
  assert!(!opts.data_append_mode);
}

/// eng-66：index cache 字节预算走 fkv 扩展键，两节均可解析。
#[test]
fn index_cache_bytes_ext_key() {
  let root = TempRoot::new("fkv-index-cache");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nindex_cache_bytes = 16MB\n[permanent]\nindex_cache_bytes = 1gb\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  // 后出现的键覆盖前值：最终为 1 GiB。
  assert_eq!(opts.index_cache_bytes, fkv_config::GB);
}

/// eng-69c：聚合预算键与每表预算各自解析；`0` = 跟随 `buffer_pool_size`。
#[test]
fn index_cache_total_bytes_ext_key() {
  let root = TempRoot::new("fkv-index-total");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nindex_cache_total_bytes = 16MB\n[permanent]\nindex_cache_total_bytes = 0\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  // 后出现的键覆盖前值：0 原样保留（= 跟随 buffer_pool_size）。
  assert_eq!(opts.index_cache_total_bytes, 0);
  // 两键互不干扰：每表预算仍是默认值（0 = 跟随 buffer_pool_size）。
  assert_eq!(opts.index_cache_bytes, 0);
}

/// 默认值唯一来源：空配置加载后与 `Default::default()` 完全一致；
/// 派生字段（权重 / lut）没有 INI 键，写了也不落字段（eng-72）。
#[test]
fn ini_empty_keeps_defaults_and_ignores_derived_keys() {
  let root = TempRoot::new("fkv-derived");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\n[permanent]\nstore_path_weights = 1, 2\nstore_path_lut = 3\nindex_cache_bytes = 16MB\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  let expected = KvOption {
    index_cache_bytes: 16 * fkv_config::MB,
    ..KvOption::default()
  };
  assert_eq!(opts, expected);
  assert!(opts.store_path_weights.is_empty());
  assert!(opts.store_path_lut_li.is_empty());
}

/// 12c：`store_path` 多根目录生成 LUT（分区 → 根序号），等权时交替分布；
/// 显式权重按槽位连续填充；单根保持恒 0。
#[test]
fn validate_builds_store_path_lut() {
  // 等权（未给权重）：补 1，LUT [0, 1]。
  let mut opts = KvOption {
    store_path: vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()],
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.store_path_weights, vec![1, 1]);
  assert_eq!(opts.store_path_lut_li, vec![0, 1]);

  // 显式权重：前 3 槽 0、后 7 槽 1（对齐 C++ ComputeStorePathLut）。
  let mut opts = KvOption {
    store_path: vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()],
    store_path_weights: vec![3, 7],
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.store_path_lut_li, vec![0, 0, 0, 1, 1, 1, 1, 1, 1, 1]);

  // gcd 归一：4:6 → 2:3，共 5 槽。
  let mut opts = KvOption {
    store_path: vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()],
    store_path_weights: vec![4, 6],
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.store_path_lut_li, vec![0, 0, 1, 1, 1]);

  // 单根：所有分区落 0 号根，行为与单 store_path 一致。
  let mut opts = KvOption {
    store_path: vec!["/tmp/a".to_owned()],
    ..KvOption::default()
  };
  opts.validate().unwrap();
  assert_eq!(opts.store_path_lut_li, vec![0]);

  // 权重 0 / 个数不符 / 只给权重不给路径：启动前拒绝。
  for bad in [
    KvOption {
      store_path: vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()],
      store_path_weights: vec![1, 0],
      ..KvOption::default()
    },
    KvOption {
      store_path: vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()],
      store_path_weights: vec![1],
      ..KvOption::default()
    },
    KvOption {
      store_path_weights: vec![1],
      ..KvOption::default()
    },
  ] {
    let mut opts = bad;
    assert_eq!(opts.validate().unwrap_err(), KvError::InvalidArgs);
  }
}

/// LUT 权重和超过上限时等比缩放；缩放下界为 1，保证每个根仍有槽位，
/// 取整后仍超出时逐个扣槽，返回长度恒 `<= max_entries`。
#[test]
fn store_path_lut_scales_to_cap() {
  let lut = fkv_config::compute_store_path_lut_capped(&[10, 1], 3);
  // gcd=1，total=11 > 3 → scale=4 → 权重 [2, 1]，LUT [0, 0, 1]。
  assert_eq!(lut, vec![0, 0, 1]);
  // 每根保底 1 槽会让取整后超出上限（缩放后 [3, 1, 1] 共 5 槽）：
  // 从最大权重扣到 4 槽，仍保持「重者多槽」。
  let lut = fkv_config::compute_store_path_lut_capped(&[100, 1, 1], 4);
  assert_eq!(lut, vec![0, 0, 1, 2]);
  // 上限比根数还小：每根最多 1 槽，只保留前 `max_entries` 个根。
  let lut = fkv_config::compute_store_path_lut_capped(&[5, 5, 5], 2);
  assert_eq!(lut, vec![0, 1]);
  for (weight_li, cap) in [
    (vec![100, 1, 1], 4),
    (vec![7, 5, 3, 2], 6),
    (vec![1_000_000, 1], 8),
    (vec![3, 7], 1),
  ] {
    assert!(
      fkv_config::compute_store_path_lut_capped(&weight_li, cap).len() <= cap,
      "权重 {weight_li:?} 上限 {cap} 不得超长"
    );
  }
  assert_eq!(fkv_config::compute_store_path_lut_capped(&[], 3), vec![]);
  assert_eq!(fkv_config::compute_store_path_lut_capped(&[1], 0), vec![]);
}

/// eng-34b：段预算默认 20us，与轮预算 `max_processing_time_us` 独立可配。
#[test]
fn yield_budget_ext_key() {
  let root = TempRoot::new("fkv_yield_budget");
  let path = root.join("config.ini");
  write(
    &path,
    "[run]\nyield_budget_us = 5\n[permanent]\nyield_budget_us = 0\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.yield_budget_us, 0);
}

/// 显式 `pages_per_file_shift` 不被派生的 `data_file_size` 覆盖。
#[test]
fn ini_explicit_shift_wins_over_data_file_size() {
  let root = TempRoot::new("fkv_shift");
  let path = root.join("config.ini");
  for text in [
    "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\npages_per_file_shift = 5\ndata_file_size = 8MB\n",
    "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\ndata_file_size = 8MB\npages_per_file_shift = 5\n",
  ] {
    write(&path, text).unwrap();
    let mut opts = KvOption::default();
    opts.load_from_ini(&path).unwrap();
    assert_eq!(opts.pages_per_file_shift, 5);
  }
}

/// `data_file_size = data_page_size << shift` 移出 u64 时必须拒绝，
/// 不能静默回绕成 0（4KB 页在 shift=52 时恰好 2^64）。
#[test]
fn validate_rejects_data_file_size_overflow() {
  let mut opts = KvOption {
    pages_per_file_shift: 52,
    ..KvOption::default()
  };
  assert_eq!(opts.validate(), Err(KvError::InvalidArgs));
  // 边界内仍合法：4KB << 51 == 2^63。
  opts.pages_per_file_shift = 51;
  assert!(opts.validate().is_ok());
}

/// `overflow_pointers` 合法区间是 `1..=MAX_OVERFLOW_POINTERS`（对齐 C++
/// `kv_options.h` 的 128 上限）。
#[test]
fn validate_overflow_pointers_boundaries() {
  use fkv_types::MAX_OVERFLOW_POINTERS;

  let mut opts = KvOption {
    overflow_pointers: 0,
    ..KvOption::default()
  };
  assert_eq!(opts.validate(), Err(KvError::InvalidArgs));
  opts.overflow_pointers = MAX_OVERFLOW_POINTERS;
  assert!(opts.validate().is_ok());
  opts.overflow_pointers = MAX_OVERFLOW_POINTERS + 1;
  assert_eq!(opts.validate(), Err(KvError::InvalidArgs));
}

/// gap-06：两个数据面超时键是 `Ext` 键（`[run]` / `[permanent]` 均接受），
/// 默认 `0`。生效值按模式解析：Local / Standby 不设（旧行为），Cloud 未配置时
/// 回退 S3 重试窗口量级，显式值两侧一致优先。
#[test]
fn request_timeout_keys_parse_and_resolve_by_mode() {
  let opts = KvOption::default();
  assert_eq!(opts.request_timeout_us, 0);
  assert_eq!(opts.request_queue_timeout_us, 0);
  assert_eq!(opts.request_timeout(), None);
  assert_eq!(opts.queue_timeout(), None);

  let root = TempRoot::new("fkv_request_timeout");
  let path = root.join("config.ini");
  // Ext：写在 [run] 生效。
  write(
    &path,
    "[run]\nrequest_timeout_us = 1234\nrequest_queue_timeout_us = 5678\n[permanent]\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.request_timeout(), Some(Duration::from_micros(1234)));
  assert_eq!(opts.queue_timeout(), Some(Duration::from_micros(5678)));

  // Ext：写在 [permanent] 同样生效。
  write(
    &path,
    "[run]\n[permanent]\nrequest_timeout_us = 42\nrequest_queue_timeout_us = 43\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.request_timeout_us, 42);
  assert_eq!(opts.request_queue_timeout_us, 43);

  // Cloud 未配置：回退到 S3 重试窗口量级。
  let cloud = KvOption {
    cloud_store_path: "bucket/prefix".to_owned(),
    ..KvOption::default()
  };
  assert_eq!(
    cloud.request_timeout(),
    Some(Duration::from_micros(DEFAULT_CLOUD_REQUEST_TIMEOUT_US))
  );
  assert_eq!(
    cloud.queue_timeout(),
    Some(Duration::from_micros(DEFAULT_CLOUD_QUEUE_TIMEOUT_US))
  );

  // Cloud 显式值优先；cloud 与 standby 双开关按 validate 后的终态（Cloud）解析。
  let explicit = KvOption {
    cloud_store_path: "bucket/prefix".to_owned(),
    enable_local_standby: true,
    request_timeout_us: 7,
    request_queue_timeout_us: 9,
    ..KvOption::default()
  };
  assert_eq!(explicit.request_timeout(), Some(Duration::from_micros(7)));
  assert_eq!(explicit.queue_timeout(), Some(Duration::from_micros(9)));

  // Standby（本地存储）未配置：保持旧行为。
  let standby = KvOption {
    enable_local_standby: true,
    store_path: vec!["/tmp/fkv".to_owned()],
    ..KvOption::default()
  };
  assert_eq!(standby.request_timeout(), None);
  assert_eq!(standby.queue_timeout(), None);
}
