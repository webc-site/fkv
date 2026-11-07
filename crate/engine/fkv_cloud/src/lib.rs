//! 云 overlay：本地 cache 为第一层，对象存储为 durable 副本。
//!
//! data 以**整文件**为单位上传 / 下载；manifest 也是整对象。
//! 本地 cache 按字节预算做 LRU 淘汰，dirty 文件在上传完成前不淘汰。
//!
//! 淘汰只做内存摘链 / 记账；本地文件删除进入待删队列，按预算的 1/8
//! 批量 [`CloudStore::drain_evictions`]（flush 时全量 drain），读写路径不再
//! 逐文件同步 unlink。预算记账在摘链时立即扣减，删除失败项回队重试。

use core::cell::{Cell, RefCell};
use std::sync::Arc;

use fkv_metrics::ShardMetric;
use fkv_types::{BranchFileRange, FileId, TableIdent};
use gxhash::{HashMap, HashSet};

use crate::{
  cache::CacheState,
  download::{DownloadKey, InflightDownload},
  group::OwnerLease,
  object_key::FileIdentity,
};

mod cache;
mod download;
mod evict;
mod file_io;
mod group;
mod listing;
mod manifest;
mod object_key;
mod page_io;
mod prewarm;
mod segment;
mod segment_io;
mod store;
mod term;

pub use cache::EVICT_PENDING_FILE_MAX;
pub use group::CacheGroup;
pub use listing::{CLOUD_LIST_PAGE_KEYS, FilePage, PartitionPage, RemoteFileEntry};

#[derive(Debug)]
pub struct CloudStore<I, O> {
  local: I,
  remote: O,
  prefix: String,
  pages_per_file_shift: u8,
  segment_size: usize,
  segments_per_file_shift: u8,
  /// 本地 cache 预算组与 owner 租约：默认本实例独占一组；`with_cache_group`
  /// 可与其他实例共享。租约 `Drop` 时注销本实例的脏登记。
  owner: OwnerLease,
  metrics: Option<Arc<ShardMetric>>,
  state: RefCell<CacheState>,
  identity: RefCell<FileIdentity>,
  /// 每表 per-file `(branch, term)` 归属链（对齐 C++ `BranchFileMapping`），
  /// replay 后由 Store 同步；旧 term / 父 branch 的 data 对象按写时 key 读取。
  file_range_map: RefCell<HashMap<TableIdent, Vec<BranchFileRange>>>,
  /// 是否复用启动前已存在的本地 cache（对齐 `allow_reuse_local_caches`）。
  /// `false` 时按表懒恢复会清空该表的本地 data / segment。
  reuse_local_caches: bool,
  /// 只读实例（预热 / 只读副本）：照常安装 segment 文件，但**不参与** scheme
  /// 回填（见 [`Self::enable_segment_marker`] 的跨进程边界）。
  ///
  /// 跨进程共用同一本地 root 时看不到对方的写现场（`CacheGroup` 只在同一
  /// 运行时内可见），回填会把对方半装文件按旧尾槽规则盖成「完整」。多进程
  /// 部署必须让该分区的写实例独占回填，预热 / 只读实例置 `true`。
  read_only: bool,
  /// 已完成本地 cache 记账恢复的表；按表懒扫描，避免每次预热重复列目录。
  restored_table_set: RefCell<HashSet<TableIdent>>,
  /// 已确认**存在**表级 segment scheme 标记的表。
  ///
  /// 只缓存肯定存在的结论；不存在仍每次查盘（共用 `local_root` 的另一实例
  /// 可能刚建立 scheme）。restore 每表要判 N 个段，缓存后 scheme 读取从 N 次
  /// 降到 1 次；`drop` / `invalidate` 删除 scheme 时必须同步移除。
  segment_scheme_set: RefCell<HashSet<TableIdent>>,
  /// 在飞整文件下载；后到者等待同一结果，避免并发重复下载与 `<file>.tmp` 争用。
  inflight_download: RefCell<HashMap<DownloadKey, InflightDownload>>,
  /// 在飞下载世代号分配器：每次登记取一个新世代，旧 owner 完成时按世代核对。
  download_generation: Cell<u64>,
  /// 本实例只是「借用」的本地文件（另一实例 / shard 已写好的 data/segment）。
  /// 驱逐时只注销记账，不 unlink 共享路径：否则 prewarm 会把 shard 尚未 flush
  /// 的脏文件当自己的 clean cache 删掉。
  foreign_file_set: RefCell<HashSet<(TableIdent, bool, FileId)>>,
  /// 驱逐代数：`evict_table` 时递增。owner 在下载开始前取值，落盘 / 登记前
  /// 复核；不一致说明期间发生过驱逐（含本表 detach / Drop），必须撤销本次
  /// 下载结果，不能把已驱逐的表重新 `upsert_resident`（否则 reopen 会按本地
  /// 残留当权威）。全局一个计数而不是按表记，避免为每张表长期保留代数；
  /// 代价是其它表的驱逐会让在飞下载多失败一次（`TryAgain`，可重试）。
  evict_epoch: Cell<u64>,
}

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod object_key_tests;
#[cfg(test)]
mod segment_tests;
#[cfg(test)]
mod test_support;
