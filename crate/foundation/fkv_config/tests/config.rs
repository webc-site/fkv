use std::{
  env::temp_dir,
  fs::{remove_file, write},
  process::id,
  time::Duration,
};

use fkv_config::{DEFAULT_CLOUD_QUEUE_TIMEOUT_US, DEFAULT_CLOUD_REQUEST_TIMEOUT_US, KvOption};
use fkv_error::KvError;
use fkv_types::StoreMode;

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

  let path = temp_dir().join(format!("fkv_write_limits_{}.ini", id()));
  write(
    &path,
    "[run]\nnum_threads = 1\nmax_inflight_write = 17\nmax_write_batch_pages = 3\n[permanent]\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.max_inflight_write, 17);
  assert_eq!(opts.max_write_batch_pages, 3);
  remove_file(&path).unwrap();
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

/// fmt-18：data / index 两个 restart 旋钮独立解析，互不影响。
#[test]
fn ini_restart_intervals_are_independent() {
  let path = temp_dir().join(format!("fkv_restart_{}.ini", id()));
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
  remove_file(path).unwrap();
}

#[test]
fn load_ini() {
  let path = temp_dir().join(format!("fkv_options_{}.ini", id()));
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
  remove_file(path).unwrap();
}

/// 键只在所属节内生效；[run] 里的 [permanent] 键被忽略。
#[test]
fn ini_section_scope() {
  let path = temp_dir().join(format!("fkv_section_{}.ini", id()));
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
  remove_file(path).unwrap();
}

/// 缺 [run] 或 [permanent] 节报错（对齐 C++ 返回 -2）。
#[test]
fn ini_missing_section_rejected() {
  let path = temp_dir().join(format!("fkv_missing_{}.ini", id()));
  write(&path, "[run]\nnum_threads = 4\n").unwrap();
  let mut opts = KvOption::default();
  assert_eq!(opts.load_from_ini(&path).unwrap_err(), KvError::InvalidArgs);
  remove_file(path).unwrap();
}

/// store_path 支持 `path1,path2:w1,w2` 内联权重（对齐 C++ `ParseStorePathListWithWeights`）。
#[test]
fn ini_store_path_inline_weights() {
  let path = temp_dir().join(format!("fkv_weights_{}.ini", id()));
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
  remove_file(path).unwrap();
}

/// data_file_size 非二次幂倍向下取整，小于 page size 回退单页（不报错）。
#[test]
fn ini_data_file_size_rounding() {
  let path = temp_dir().join(format!("fkv_filesize_{}.ini", id()));
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
  remove_file(path).unwrap();
}

/// 单位文法对齐 C++ `ParseSizeWithUnit`：只认两位尾缀 KB/MB/GB/TB，无单位按字节。
#[test]
fn ini_size_units_match_cpp() {
  let path = temp_dir().join(format!("fkv_units_{}.ini", id()));
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
  remove_file(path).unwrap();
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
  let path = temp_dir().join(format!("fkv-index-cache-{}.ini", id()));
  write(
    &path,
    "[run]\nindex_cache_bytes = 16MB\n[permanent]\nindex_cache_bytes = 1gb\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  // 后出现的键覆盖前值：最终为 1 GiB。
  assert_eq!(opts.index_cache_bytes, fkv_config::GB);
  let _ = remove_file(path);
}

/// eng-69c：聚合预算键与每表预算各自解析，`0` 表示不限制。
#[test]
fn index_cache_total_bytes_ext_key() {
  let path = temp_dir().join(format!("fkv-index-total-{}.ini", id()));
  write(
    &path,
    "[run]\nindex_cache_total_bytes = 16MB\n[permanent]\nindex_cache_total_bytes = 0\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  // 后出现的键覆盖前值：0 原样保留（= 不限制聚合）。
  assert_eq!(opts.index_cache_total_bytes, 0);
  // 两键互不干扰：每表预算仍是默认值。
  assert_eq!(
    opts.index_cache_bytes,
    fkv_config::DEFAULT_INDEX_CACHE_BYTES
  );
  let _ = remove_file(path);
}

/// 默认值唯一来源：空配置加载后与 `Default::default()` 完全一致；
/// 派生字段（权重 / lut）没有 INI 键，写了也不落字段（eng-72）。
#[test]
fn ini_empty_keeps_defaults_and_ignores_derived_keys() {
  let path = temp_dir().join(format!("fkv-derived-{}.ini", id()));
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
  remove_file(path).unwrap();
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
  assert_eq!(fkv_config::DEFAULT_YIELD_BUDGET_US, 20);

  let path = temp_dir().join(format!("fkv_yield_budget_{}.ini", id()));
  write(
    &path,
    "[run]\nyield_budget_us = 5\n[permanent]\nyield_budget_us = 0\n",
  )
  .unwrap();
  let mut opts = KvOption::default();
  opts.load_from_ini(&path).unwrap();
  assert_eq!(opts.yield_budget_us, 0);
  remove_file(&path).unwrap();
}

/// 显式 `pages_per_file_shift` 不被派生的 `data_file_size` 覆盖。
#[test]
fn ini_explicit_shift_wins_over_data_file_size() {
  let path = temp_dir().join(format!("fkv_shift_{}.ini", id()));
  for text in [
    "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\npages_per_file_shift = 5\ndata_file_size = 8MB\n",
    "[run]\nnum_threads = 1\n[permanent]\ndata_page_size = 4KB\ndata_file_size = 8MB\npages_per_file_shift = 5\n",
  ] {
    write(&path, text).unwrap();
    let mut opts = KvOption::default();
    opts.load_from_ini(&path).unwrap();
    assert_eq!(opts.pages_per_file_shift, 5);
  }
  remove_file(path).unwrap();
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

  let path = temp_dir().join(format!("fkv_request_timeout_{}.ini", id()));
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
  remove_file(&path).unwrap();

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
