//! Shard 请求 / 响应。只放纯数据，不依赖 Store / IO。

use fkv_btree::BatchEntry;
use fkv_types::{FileId, InstallOutcome, TableIdent, now_us};

/// 归档 tag：Unix 微秒、定宽 20 位十进制（u64 微秒上限恰好 20 位）。
///
/// 精度对齐 C++（`background_write.cpp:586-591` 空 tag 用微秒时间戳）；定宽是因为
/// `fkv_store` 的 `prune_archive_li` 按文件名排序决定保留顺序，C++ 的不定宽十进制
/// 在这里会让短 tag 排到长 tag 前面（排序 != 时间序）。crond 定时归档与空 tag
/// 自动生成共用此函数，保证同一分区的 tag 可比较。
pub fn archive_tag() -> String {
  format!("{:020}", now_us())
}

#[derive(Debug, Clone)]
pub enum Request {
  Put {
    tbl: TableIdent,
    batch_li: Vec<BatchEntry>,
    term: Option<u64>,
  },
  Get {
    tbl: TableIdent,
    key: Vec<u8>,
  },
  PutLarge {
    tbl: TableIdent,
    batch_li: Vec<BatchEntry>,
    term: Option<u64>,
  },
  GetLarge {
    tbl: TableIdent,
    key: Vec<u8>,
  },
  Scan {
    tbl: TableIdent,
    begin: Vec<u8>,
    end: Vec<u8>,
    max_entries: usize,
    /// 起止开闭与分页参数（`begin` / `end` 的语义由它统一描述）。
    option: ScanPageOption,
  },
  /// 重新打开表的分区。
  ///
  /// `StandbyReplica` 下先把该分区从 master 追一次再打开本地 manifest（对齐
  /// C++ `ReopenTask::Reopen` 的 `RsyncPartition` + `WaitIo`）；`ResourceMissing`
  /// 由事件循环的延迟队列重试，不在任务内 sleep。其它模式只重读当前 manifest。
  Reopen {
    tbl: TableIdent,
  },
  /// 采纳外部（远端）当前快照：Cloud 强制回源 manifest，StandbyReplica 先追赶
  /// master；远端缺失时只清本地状态。仅 Cloud / StandbyReplica 模式可用。
  /// 生产入口见 `fkv_runtime::Runtime::reopen_external`。
  ReopenExternal {
    tbl: TableIdent,
  },
  /// 文件 GC（对齐 C++ `FileGcRequest` / `WriteTask::RunFileGc`）：按 store mode
  /// 选择删除目标 —— Cloud 删远端对象（并清对应本地 cache），Local / Mem 删
  /// 本地文件。一轮同时回收 data 与 segment 文件（C++ `RunFileGc` 内部先
  /// `ExecuteCloudGC` / `ExecuteLocalGC`，两类文件都在同一轮处理）。
  ///
  /// 布局参数（`pages_per_file_shift` / `segments_per_file_shift`）由 shard 与
  /// store 配置决定，请求不携带，避免调用方传入与盘面不符的位宽。
  FileGc {
    tbl: TableIdent,
  },
  /// 只回收**本地**文件 / 本地 cache（对齐 C++ `LocalGcRequest` /
  /// `WriteTask::RunLocalFileGc` → `ExecuteLocalGC`），绝不删除远端对象。
  ///
  /// 引用集判定与 `FileGc` 完全一致，差别只在删除范围：Cloud 模式下用于清掉
  /// 本地 cache 中已无引用的 data / segment 文件，远端副本原样保留，后续读按
  /// 需回源。Local / Mem 模式下与 `FileGc` 等价。
  LocalGc {
    tbl: TableIdent,
  },
  Compact {
    tbl: TableIdent,
    file_id: FileId,
    pages_per_file_shift: u8,
  },
  /// 清空表内容但**保留表**：释放主树 / TTL 树的物理页，root / ttl_root 归零，
  /// 表仍可继续读写。对齐 C++ `TruncateRequest`。
  ///
  /// `position` 为空 → 整表清空（`Store::truncate`）；非空 → 删除所有
  /// `key >= position` 的条目（`Store::truncate_from`）。
  /// 与 `Drop` 的区别：`Truncate` 保留 manifest / 表本身，`Drop` 连表一起删。
  Truncate {
    tbl: TableIdent,
    position: Vec<u8>,
  },
  Floor {
    tbl: TableIdent,
    key: Vec<u8>,
  },
  /// 删除整表（**按分区**）：删当前 identity 的 manifest 并清空内存状态。
  /// data / segment 文件不在 `Store::drop_table` 内硬删，而由 shard 在 Drop
  /// 回执前跑一轮 `FileGc`，按全部 manifest 的 retained set 回收（对齐 C++
  /// `DropManifest` + `TryAddFileGc`），避免破坏 archive / 其它 branch 仍引用
  /// 的快照。
  ///
  /// 等义 `Store::drop_table`，也等义 C++ 的 `DropRequest`：`eloq_store.h:576` 的
  /// `DropRequest { TableIdent }` 就是 per-partition 请求，handler
  /// `BatchWriteTask::Drop()` = `DropManifest` + 清 `RootMeta` + 触发 GC；同头文件
  /// `:586` 的 `DropTableRequest` 只是 store 级把 `DropRequest` 扇出到该表的所有
  /// 分区。因此这里的 `tbl` 是单个分区；store 级「删整个表」由上层对各分区分别
  /// 下发本请求实现。
  ///
  /// **不是单 key / 前缀删除**：单 key 逻辑删除走 `Put { batch_li }` 里的
  /// `BatchEntry::delete`（对齐 C++ 用 `BatchWriteRequest` 携带 delete op）；
  /// `Store::delete` 只是该路径的便捷封装，没有独立的 delete 请求类型。
  Drop {
    tbl: TableIdent,
  },
  CleanExpired {
    tbl: TableIdent,
  },
  AttachShard {
    branch: String,
    shard_id: u32,
    term: u64,
  },
  DetachShard {
    shard_id: u32,
  },
  CreateArchive {
    tbl: TableIdent,
    tag: String,
    num_retained: u32,
  },
  /// 按 tag 重新打开归档快照（对齐 C++ `ReopenRequest::SetTag` 的读取入口）。
  ///
  /// `tag` 为空 = 重新打开当前活跃快照（等价 `Reopen`，对齐 C++「empty means
  /// reopen the latest available snapshot」）；非空 = 选中当前分支下该 tag 的
  /// 归档 manifest，replay 后提升为活跃状态（多 term 时取最高 term）。
  ReopenArchive {
    tbl: TableIdent,
    tag: String,
  },
  /// 删除指定 tag 的归档（对齐 C++ `ArchiveRequest::Action::Delete`）。
  ///
  /// 删除当前分支下该 tag 的**全部 term** 的归档 manifest；文件不存在视为成功
  /// （幂等）。空 tag 非法。活跃 manifest 与 data 文件不受影响：被删归档不再
  /// 出现在 retained set，data 文件由后续 GC 回收。
  DeleteArchive {
    tbl: TableIdent,
    tag: String,
  },
  /// Cloud 模式：把该分区的远端文件（manifest / data / segment）拉到本地 cache。
  ///
  /// 对齐 C++ `PrewarmService::Prewarm` 的「按分区触发一轮预热」语义，但这里是
  /// **同步请求**：请求在 shard 线程内完成全部下载后回执，调用方拿到成功即代表
  /// 本轮已跑完（后台周期预热仍由 `Runtime` 的 `PrewarmCrond` 负责）。
  ///
  /// 仅 `Backend::Cloud` 支持；Local / Mem / Standby 没有远端对象可拉，返回
  /// `InvalidArgs`，不静默降级为空操作（空操作会让调用方误以为 cache 已就绪）。
  Prewarm {
    tbl: TableIdent,
  },
  /// 列举该表的归档 tag（`manifest_<branch>_<term>_<tag>` 里的 `(tag, term)`）。
  ///
  /// 对齐 C++ `GlobalListArchiveTagsRequest` 的语义，只是范围收窄到单个分区：
  /// 扫该表目录下**全部 branch / term** 的归档 manifest，结果按 `(tag, term)`
  /// 升序并去重。与 `prune_archives` 的「按 tag 倒序保留最近 N 份」不同：
  /// 列举给出完整清单，保留策略只决定删谁，两者排序方向不共享。
  ///
  /// 这是控制面只读请求：不打开 / 不创建表，也不检查归档是否属于当前 branch。
  ListArchiveTags {
    tbl: TableIdent,
  },
  /// 列举远端对象（Cloud 模式）。
  ///
  /// - `tbl`：只用于路由到服务该分区的 shard；
  /// - `prefix`：**远端 key 的原始前缀**（对齐 C++ `ListObjectRequest::SetRemotePath`），
  ///   不做表目录自动拼接；空串表示列举整个 bucket；
  /// - `token` / `max_keys`：分页契约同 `fkv_object::ObjectStore::list_page`，
  ///   `next_token` 即下一页要回传的 `token`，`max_keys == 0` 表示不限数量；
  ///   列举始终递归（`ObjectStore::list_page` 没有目录分隔语义，等价 C++
  ///   `SetRecursive(true)`）。
  ///
  /// 仅 `Backend::Cloud` 支持；其它后端没有对象存储，返回 `InvalidArgs`（空列表
  /// 会被误解为「远端确实没有对象」）。
  ListObject {
    tbl: TableIdent,
    prefix: String,
    token: Option<String>,
    max_keys: usize,
  },
  /// 列举 standby master 上已有的分区。
  ///
  /// 对齐 C++ `ListStandbyPartitionRequest`：由 shard 的 standby service 向 master
  /// 来源列举目录（`fkv_standby::RemoteSource::list_table_li`）。非 standby 模式
  /// 没有该服务，返回 `InvalidArgs`。
  ListStandbyPartition,
  CreateBranch {
    tbl: TableIdent,
    branch: String,
  },
  DeleteBranch {
    tbl: TableIdent,
    branch: String,
  },
  Shutdown,
}

impl Request {
  pub fn table(&self) -> Option<&TableIdent> {
    match self {
      Self::Put { tbl, .. }
      | Self::Get { tbl, .. }
      | Self::PutLarge { tbl, .. }
      | Self::GetLarge { tbl, .. }
      | Self::Scan { tbl, .. }
      | Self::Reopen { tbl }
      | Self::ReopenExternal { tbl }
      | Self::FileGc { tbl }
      | Self::LocalGc { tbl }
      | Self::Compact { tbl, .. }
      | Self::Truncate { tbl, .. }
      | Self::Floor { tbl, .. }
      | Self::Drop { tbl }
      | Self::CleanExpired { tbl }
      | Self::CreateArchive { tbl, .. }
      | Self::ReopenArchive { tbl, .. }
      | Self::DeleteArchive { tbl, .. }
      | Self::Prewarm { tbl }
      | Self::ListArchiveTags { tbl }
      | Self::ListObject { tbl, .. }
      | Self::CreateBranch { tbl, .. }
      | Self::DeleteBranch { tbl, .. } => Some(tbl),
      Self::AttachShard { .. } | Self::DetachShard { .. } | Self::ListStandbyPartition => None,
      Self::Shutdown => None,
    }
  }

  pub fn is_shutdown(&self) -> bool {
    matches!(self, Self::Shutdown)
  }

  /// 控制面请求：携带的是 shard 身份，而不是某个分区。
  ///
  /// 这类请求必须经 `Runtime::attach_shard` / `detach_shard` / `shutdown` 发出，
  /// 它们负责记录 attach 身份与清视图；走按 `TableIdent` 路由的数据面入口会
  /// 绕过这些收尾，并按调用方的 `tbl` 清错 shard。
  ///
  /// `ListStandbyPartition` 没有 `tbl`（[`Request::table`] 回 `None`）：按调用方
  /// `tbl` 路由会打到任意 shard，而该请求固定由 shard 0 的 standby service 应答。
  pub fn is_control_plane(&self) -> bool {
    matches!(
      self,
      Self::AttachShard { .. }
        | Self::DetachShard { .. }
        | Self::ListStandbyPartition
        | Self::Shutdown
    )
  }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanExpiredReply {
  pub deleted: usize,
  pub ttl_deleted: usize,
  pub next_expire_ts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveReply {
  pub name: String,
  pub bytes: usize,
  pub removed_num: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanPageOption {
  pub begin_inclusive: bool,
  pub end_inclusive: bool,
  pub page_entries: usize,
  pub page_size: usize,
}

impl Default for ScanPageOption {
  fn default() -> Self {
    Self {
      begin_inclusive: true,
      end_inclusive: false,
      page_entries: 0,
      page_size: 0,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReply {
  pub entry_li: Vec<(Vec<u8>, Vec<u8>)>,
  pub has_remaining: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloorReply {
  pub key: Vec<u8>,
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
  pub timestamp: u64,
  pub expire_ts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchReply {
  pub name: String,
  pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeValueReply {
  pub value: Vec<u8>,
  pub metadata: Vec<u8>,
}

/// 一个归档条目：归档 manifest 名里的 `(tag, term)`。
///
/// 字段顺序 = `Ord` 顺序（先 `tag` 后 `term`），与 C++
/// `GlobalListArchiveTagsRequest::ArchiveEntry` 的比较器一致。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArchiveTagInfo {
  pub tag: String,
  pub term: u64,
}

/// 远端对象元数据（`ListObject` 回执）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
  pub key: String,
  pub etag: String,
  pub size: u64,
}

/// 一页远端对象列举结果。
///
/// 契约同 `fkv_object::ObjectStore::list_page`：`meta_li` 按 key 字节序升序，
/// `next_token` 为下一页的起始 token（本页最后一个 key）；`None` 表示已到末尾。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPage {
  pub meta_li: Vec<ObjectMeta>,
  pub next_token: Option<String>,
}

#[derive(Debug)]
pub enum Response {
  Put,
  Get(Option<Vec<u8>>),
  GetLarge(Option<LargeValueReply>),
  Scan(ScanReply),
  Reopen,
  ReopenExternal(InstallOutcome),
  /// 一轮文件 GC 删除的条目数（data + segment）。
  FileGc(usize),
  /// 一轮本地 GC 删除的条目数（data + segment，仅本地）。
  LocalGc(usize),
  Compact(usize),
  Truncate,
  Floor(FloorReply),
  Drop,
  CleanExpired(CleanExpiredReply),
  CreateArchive(ArchiveReply),
  CreateBranch(BranchReply),
  DeleteBranch(usize),
  /// 删除归档 tag：删除的 manifest 文件数（不存在为 0，幂等）。
  DeleteArchive(usize),
  /// 一轮预热完成。失败原因通过 `Err(KvError)` 返回；统计明细见
  /// `fkv_task_prewarm::PrewarmStat` / `Runtime::prewarm_report`。
  Prewarm,
  /// 该表的归档 `(tag, term)` 清单（按 `(tag, term)` 升序去重）。
  ListArchiveTags(Vec<ArchiveTagInfo>),
  /// 一页远端对象列举结果。
  ListObject(ListPage),
  /// standby master 上的分区清单。
  ListStandbyPartition(Vec<TableIdent>),
  AttachShard,
  DetachShard,
  Shutdown,
}
