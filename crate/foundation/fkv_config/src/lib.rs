//! `KvOptions` 与 eloqstore `include/kv_options.h` 对齐。
//!
//! 先落地字段、默认值、模式派生与校验；INI 解析在后续提交补齐。

use std::{fs::read_to_string, path::Path, str::FromStr};

use fkv_error::{KvError, Result};
use fkv_types::StoreMode;

pub const KB: u64 = 1 << 10;
pub const MB: u64 = 1 << 20;
pub const GB: u64 = 1 << 30;
pub const TB: u64 = 1 << 40;
pub const PAGE_ALIGN: u64 = 4096;

#[derive(Debug, Clone, PartialEq)]
pub struct KvOptions {
  pub num_threads: u16,
  pub data_page_restart_interval: u16,
  pub index_page_restart_interval: u16,
  pub init_page_count: u32,
  pub skip_verify_checksum: bool,
  pub buffer_pool_size: u64,
  pub enable_data_page_cache: bool,
  pub root_meta_cache_size: u64,
  pub manifest_limit: u32,
  pub fd_limit: u32,
  pub io_queue_size: u32,
  pub max_inflight_write: u32,
  pub max_write_batch_pages: u32,
  pub coroutine_stack_size: u32,
  pub num_retained_archives: u32,
  pub archive_interval_secs: u32,
  pub max_archive_tasks: u16,
  pub max_global_request_batch: u32,
  pub file_amplify_factor: u8,
  pub segment_file_amplify_factor: u8,
  pub segment_compact_yield_every: u32,
  pub local_space_limit: u64,
  pub reserve_space_ratio: u16,
  pub max_cloud_concurrency: u32,
  pub max_write_concurrency: u32,
  pub cloud_request_threads: u16,
  pub standby_max_concurrency: u16,
  pub direct_io_buffer_pool_size: u32,
  pub write_buffer_size: u64,
  pub non_page_io_batch_size: u64,
  pub write_buffer_ratio: f64,
  pub allow_reuse_local_caches: bool,
  pub chunk_size: u32,
  pub segment_size: u32,
  pub gc_global_mem_size_per_shard: u64,
  pub pinned_tail_scratch_slots: u16,
  pub segments_per_file_shift: u8,

  pub store_path: Vec<String>,
  pub store_path_weights: Vec<u64>,
  pub store_path_lut: Vec<u32>,
  pub cloud_store_path: String,
  pub cloud_provider: String,
  pub cloud_endpoint: String,
  pub cloud_region: String,
  pub cloud_access_key: String,
  pub cloud_secret_key: String,
  pub cloud_auto_credentials: bool,
  pub enable_local_standby: bool,
  pub standby_master_addr: String,
  pub standby_master_store_paths: Vec<String>,
  pub standby_master_store_path_weights: Vec<u64>,
  pub standby_master_store_path_lut: Vec<u32>,
  pub cloud_verify_ssl: bool,

  pub data_page_size: u32,
  pub pages_per_file_shift: u8,
  pub overflow_pointers: u8,
  pub data_append_mode: bool,
  pub enable_compression: bool,
  pub zstd_compression_level: i32,
  pub prewarm_cloud_cache: bool,
  pub prewarm_task_count: u16,
  pub auto_reopen_retry_times: u8,
  pub auto_oom_retry_times: u8,
  pub auto_reopen_pending_time_us: u64,
}

impl Default for KvOptions {
  fn default() -> Self {
    Self {
      num_threads: 1,
      data_page_restart_interval: 16,
      index_page_restart_interval: 16,
      init_page_count: 1 << 15,
      skip_verify_checksum: false,
      buffer_pool_size: 128 * MB,
      enable_data_page_cache: false,
      root_meta_cache_size: GB,
      manifest_limit: 8 * MB as u32,
      fd_limit: 10_000,
      io_queue_size: 4096,
      max_inflight_write: 32 << 10,
      max_write_batch_pages: 32,
      coroutine_stack_size: 32 * KB as u32,
      num_retained_archives: 100,
      archive_interval_secs: 0,
      max_archive_tasks: 256,
      max_global_request_batch: 1000,
      file_amplify_factor: 2,
      segment_file_amplify_factor: 2,
      segment_compact_yield_every: 8,
      local_space_limit: TB,
      reserve_space_ratio: 100,
      max_cloud_concurrency: 20,
      max_write_concurrency: 0,
      cloud_request_threads: 1,
      standby_max_concurrency: 1,
      direct_io_buffer_pool_size: 16,
      write_buffer_size: MB,
      non_page_io_batch_size: MB,
      write_buffer_ratio: 0.05,
      allow_reuse_local_caches: false,
      chunk_size: 256,
      segment_size: 256 * KB as u32,
      gc_global_mem_size_per_shard: 32 * MB,
      pinned_tail_scratch_slots: 8,
      segments_per_file_shift: 7,

      store_path: Vec::new(),
      store_path_weights: Vec::new(),
      store_path_lut: Vec::new(),
      cloud_store_path: String::new(),
      cloud_provider: "aws".to_owned(),
      cloud_endpoint: String::new(),
      cloud_region: "us-east-1".to_owned(),
      cloud_access_key: "minioadmin".to_owned(),
      cloud_secret_key: "minioadmin".to_owned(),
      cloud_auto_credentials: false,
      enable_local_standby: false,
      standby_master_addr: String::new(),
      standby_master_store_paths: Vec::new(),
      standby_master_store_path_weights: Vec::new(),
      standby_master_store_path_lut: Vec::new(),
      cloud_verify_ssl: false,

      data_page_size: 4 * KB as u32,
      pages_per_file_shift: 18,
      overflow_pointers: 16,
      data_append_mode: false,
      enable_compression: false,
      zstd_compression_level: 3,
      prewarm_cloud_cache: false,
      prewarm_task_count: 3,
      auto_reopen_retry_times: 10,
      auto_oom_retry_times: 5,
      auto_reopen_pending_time_us: 10_000_000,
    }
  }
}

impl KvOptions {
  pub fn derive_mode(&self) -> StoreMode {
    if !self.cloud_store_path.is_empty() {
      StoreMode::Cloud
    } else if self.enable_local_standby {
      if self.standby_master_addr.is_empty() {
        StoreMode::StandbyMaster
      } else {
        StoreMode::StandbyReplica
      }
    } else {
      StoreMode::Local
    }
  }

  pub fn data_file_size(&self) -> u64 {
    u64::from(self.data_page_size) << self.pages_per_file_shift
  }

  pub fn segment_file_size(&self) -> u64 {
    u64::from(self.segment_size) << self.segments_per_file_shift
  }

  pub fn file_page_offset_mask(&self) -> u64 {
    (1u64 << self.pages_per_file_shift) - 1
  }

  /// 对齐 `ValidateOptions`：校验失败返回 `InvalidArgs`，必要时 rewrite 字段。
  pub fn validate(&mut self) -> Result<()> {
    while self.cloud_store_path.ends_with('/') {
      self.cloud_store_path.pop();
    }
    if self.num_threads == 0
      || self.max_inflight_write == 0
      || self.max_global_request_batch == 0
      || self.max_write_batch_pages == 0
      || self.data_page_size == 0
      || !u64::from(self.data_page_size).is_multiple_of(PAGE_ALIGN)
      || self.segment_size == 0
      || !u64::from(self.segment_size).is_multiple_of(PAGE_ALIGN)
      || !u64::from(self.coroutine_stack_size).is_multiple_of(PAGE_ALIGN)
      || self.non_page_io_batch_size == 0
      || !self.non_page_io_batch_size.is_multiple_of(PAGE_ALIGN)
    {
      return Err(KvError::InvalidArgs);
    }
    if self.overflow_pointers == 0 || self.overflow_pointers > 128 {
      return Err(KvError::InvalidArgs);
    }
    if (self.write_buffer_size != 0 || self.write_buffer_ratio != 0.0)
      && (self.write_buffer_size == 0
        || !(0.0..1.0).contains(&self.write_buffer_ratio)
        || !self.write_buffer_size.is_multiple_of(PAGE_ALIGN))
    {
      return Err(KvError::InvalidArgs);
    }
    if !self.cloud_store_path.is_empty() {
      self.enable_local_standby = false;
      self.standby_master_addr.clear();
      self.standby_master_store_paths.clear();
      self.standby_master_store_path_weights.clear();
      self.standby_master_store_path_lut.clear();
    }
    if !self.enable_local_standby && !self.standby_master_addr.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    if self.enable_local_standby {
      if self.store_path.is_empty() {
        return Err(KvError::InvalidArgs);
      }
      if !self.store_path_weights.is_empty()
        && self.store_path_weights.len() != self.store_path.len()
      {
        return Err(KvError::InvalidArgs);
      }
      if self.store_path_weights.contains(&0) {
        return Err(KvError::InvalidArgs);
      }
    }
    Ok(())
  }
}

impl KvOptions {
  /// 读取 eloqstore 的 `.ini`；`[section]` 只做忽略，key 覆盖默认值。
  pub fn load_from_ini(&mut self, path: impl AsRef<Path>) -> Result<()> {
    let text = read_to_string(path).map_err(|_| KvError::IoFail)?;
    let mut data_file_size = None;
    for raw in text.lines() {
      let line = raw.trim();
      if line.is_empty() || line.starts_with('#') || line.starts_with(';') || line.starts_with('[')
      {
        continue;
      }
      let Some((key, value)) = line.split_once('=') else {
        continue;
      };
      let key = key.trim();
      let value = value.trim();
      match key {
        "num_threads" => self.num_threads = parse_num(value)?,
        "data_page_restart_interval" => self.data_page_restart_interval = parse_num(value)?,
        "index_page_restart_interval" => self.index_page_restart_interval = parse_num(value)?,
        "init_page_count" => self.init_page_count = parse_num(value)?,
        "skip_verify_checksum" => self.skip_verify_checksum = parse_bool(value)?,
        "buffer_pool_size" => self.buffer_pool_size = parse_size(value)?,
        "enable_data_page_cache" => self.enable_data_page_cache = parse_bool(value)?,
        "root_meta_cache_size" => self.root_meta_cache_size = parse_size(value)?,
        "manifest_limit" => self.manifest_limit = size_to_u32(parse_size(value)?)?,
        "fd_limit" => self.fd_limit = parse_num(value)?,
        "io_queue_size" => self.io_queue_size = parse_num(value)?,
        "max_inflight_write" => self.max_inflight_write = parse_num(value)?,
        "max_write_batch_pages" => self.max_write_batch_pages = parse_num(value)?,
        "coroutine_stack_size" => self.coroutine_stack_size = size_to_u32(parse_size(value)?)?,
        "num_retained_archives" => self.num_retained_archives = parse_num(value)?,
        "archive_interval_secs" => self.archive_interval_secs = parse_num(value)?,
        "max_archive_tasks" => self.max_archive_tasks = parse_num(value)?,
        "max_global_request_batch" => self.max_global_request_batch = parse_num(value)?,
        "file_amplify_factor" => self.file_amplify_factor = parse_num(value)?,
        "segment_file_amplify_factor" => self.segment_file_amplify_factor = parse_num(value)?,
        "segment_compact_yield_every" => self.segment_compact_yield_every = parse_num(value)?,
        "local_space_limit" => self.local_space_limit = parse_size(value)?,
        "reserve_space_ratio" => self.reserve_space_ratio = parse_num(value)?,
        "max_cloud_concurrency" => self.max_cloud_concurrency = parse_num(value)?,
        "max_write_concurrency" => self.max_write_concurrency = parse_num(value)?,
        "cloud_request_threads" => self.cloud_request_threads = parse_num(value)?,
        "standby_max_concurrency" => self.standby_max_concurrency = parse_num(value)?,
        "direct_io_buffer_pool_size" => self.direct_io_buffer_pool_size = parse_num(value)?,
        "write_buffer_size" => self.write_buffer_size = parse_size(value)?,
        "non_page_io_batch_size" => self.non_page_io_batch_size = parse_size(value)?,
        "write_buffer_ratio" => self.write_buffer_ratio = parse_f64(value)?,
        "allow_reuse_local_caches" => self.allow_reuse_local_caches = parse_bool(value)?,
        "chunk_size" => self.chunk_size = parse_num(value)?,
        "segment_size" => self.segment_size = size_to_u32(parse_size(value)?)?,
        "gc_global_mem_size_per_shard" => self.gc_global_mem_size_per_shard = parse_size(value)?,
        "pinned_tail_scratch_slots" => self.pinned_tail_scratch_slots = parse_num(value)?,
        "segments_per_file_shift" => self.segments_per_file_shift = parse_num(value)?,
        "pages_per_file_shift" => self.pages_per_file_shift = parse_num(value)?,
        "overflow_pointers" => self.overflow_pointers = parse_num(value)?,
        "data_append_mode" => self.data_append_mode = parse_bool(value)?,
        "enable_compression" => self.enable_compression = parse_bool(value)?,
        "zstd_compression_level" => self.zstd_compression_level = parse_num(value)?,
        "prewarm_cloud_cache" => self.prewarm_cloud_cache = parse_bool(value)?,
        "prewarm_task_count" => self.prewarm_task_count = parse_num(value)?,
        "auto_reopen_retry_times" => self.auto_reopen_retry_times = parse_num(value)?,
        "auto_oom_retry_times" => self.auto_oom_retry_times = parse_num(value)?,
        "auto_reopen_pending_time_us" => self.auto_reopen_pending_time_us = parse_num(value)?,
        "store_path" => {
          self.store_path = split_list(value);
        }
        "store_path_weights" => self.store_path_weights = parse_u64_list(value)?,
        "cloud_store_path" => self.cloud_store_path = value.to_owned(),
        "cloud_provider" => self.cloud_provider = value.to_owned(),
        "cloud_endpoint" => self.cloud_endpoint = value.to_owned(),
        "cloud_region" => self.cloud_region = value.to_owned(),
        "cloud_access_key" => self.cloud_access_key = value.to_owned(),
        "cloud_secret_key" => self.cloud_secret_key = value.to_owned(),
        "cloud_auto_credentials" => self.cloud_auto_credentials = parse_bool(value)?,
        "enable_local_standby" => self.enable_local_standby = parse_bool(value)?,
        "standby_master_addr" => self.standby_master_addr = value.to_owned(),
        "standby_master_store_paths" => self.standby_master_store_paths = split_list(value),
        "cloud_verify_ssl" => self.cloud_verify_ssl = parse_bool(value)?,
        "data_page_size" => self.data_page_size = size_to_u32(parse_size(value)?)?,
        "data_file_size" => data_file_size = Some(parse_size(value)?),
        _ => {}
      }
    }
    if let Some(size) = data_file_size {
      let page_size = u64::from(self.data_page_size);
      if page_size == 0 || size % page_size != 0 {
        return Err(KvError::InvalidArgs);
      }
      let pages = size / page_size;
      if !pages.is_power_of_two() {
        return Err(KvError::InvalidArgs);
      }
      self.pages_per_file_shift = pages.trailing_zeros() as u8;
    }
    Ok(())
  }
}

fn parse_bool(value: &str) -> Result<bool> {
  match value.to_ascii_lowercase().as_str() {
    "1" | "true" | "yes" | "on" => Ok(true),
    "0" | "false" | "no" | "off" => Ok(false),
    _ => Err(KvError::InvalidArgs),
  }
}

fn parse_num<T: FromStr>(value: &str) -> Result<T> {
  value.parse().map_err(|_| KvError::InvalidArgs)
}

fn parse_f64(value: &str) -> Result<f64> {
  value.parse().map_err(|_| KvError::InvalidArgs)
}

fn size_to_u32(value: u64) -> Result<u32> {
  u32::try_from(value).map_err(|_| KvError::InvalidArgs)
}

fn parse_size(value: &str) -> Result<u64> {
  let value = value.trim();
  let split = value
    .find(|ch: char| !ch.is_ascii_digit())
    .unwrap_or(value.len());
  let (num, unit) = value.split_at(split);
  let num: u64 = num.parse().map_err(|_| KvError::InvalidArgs)?;
  let mult = match unit.trim().to_ascii_lowercase().as_str() {
    "" | "b" => 1,
    "k" | "kb" => KB,
    "m" | "mb" => MB,
    "g" | "gb" => GB,
    "t" | "tb" => TB,
    _ => return Err(KvError::InvalidArgs),
  };
  num.checked_mul(mult).ok_or(KvError::InvalidArgs)
}

fn split_list(value: &str) -> Vec<String> {
  value
    .split(',')
    .map(str::trim)
    .filter(|item| !item.is_empty())
    .map(str::to_owned)
    .collect()
}

fn parse_u64_list(value: &str) -> Result<Vec<u64>> {
  split_list(value)
    .iter()
    .map(|item| item.parse().map_err(|_| KvError::InvalidArgs))
    .collect()
}
