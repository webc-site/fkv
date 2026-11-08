//! Shard 请求 / 响应。只放纯数据，不依赖 Store / IO。

use std::time::SystemTime;

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
  /// 点读单个 key 的完整 value。
  ///
  /// 只返回 value（`Response::Get`）；需要 entry 版本时间戳 / `expire_ts` 或
  /// large value 的 metadata 时，进程内用 `fkv_store::Store::get_entry` /
  /// `fkv_sdk::KvStore::get_entry`（对齐 C++ `ReadRequest` 的 `value_` /
  /// `ts_` / `expire_ts_` 与默认 metadata-only 形态）。
  ///
  /// large value 返回 `KvError::LargeValueUnsupported`：`entry.value` 是
  /// segment 指针编码，不能当用户数据透传（issue 15）。完整大 value 用
  /// [`Self::GetLarge`]。
  Get {
    tbl: TableIdent,
    key: Vec<u8>,
  },
  PutLarge {
    tbl: TableIdent,
    batch_li: Vec<BatchEntry>,
    term: Option<u64>,
  },
  /// 读取大 value：返回完整 value + metadata trailer（`Response::GetLarge`）。
  ///
  /// 没有单独的 `large_value_only` 模式：C++ `ReadRequest::large_value_only_`
  /// 只是「不把 metadata 写进 `value_`」的省拷贝开关，不改变可见数据；Rust
  /// 始终返回 metadata 字段（trailer 上界 64 KiB），调用方需要时忽略即可。
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
  ///
  /// `clean` 对齐 C++ `ReopenRequest::SetClean`：`StandbyReplica` 下 master 已
  /// 删除该分区（同步返回 `NotFound` / `ResourceMissing`）时，本地 manifest 已
  /// 由同步路径删除，本请求按「清理成功」回 `Response::Reopen`，不再上抛错误。
  /// 只有 standby 全局重开对「仅本地存在」的分区置 `true`；普通 reopen /
  /// auto-reopen 一律 `false`，缺失仍回 `ResourceMissing`。
  Reopen {
    tbl: TableIdent,
    clean: bool,
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
  ///
  /// `clean` 对齐 C++ `ReopenRequest::SetClean`：只有 `StandbyReplica` 的**全局**
  /// 重开对「master 已删、仅本地存在」的分区置 `true`，同步返回 `NotFound` /
  /// `ResourceMissing` 时按清理成功回执。显式 `reopen_archive` 一律 `false`：
  /// 非空 tag 是对确定历史快照的请求，快照缺失仍回 `ResourceMissing`（两侧
  /// 差异见 eloqstore2fkv/COMPAT_BASELINE「缺失快照的重开语义」）。
  ReopenArchive {
    tbl: TableIdent,
    tag: String,
    clean: bool,
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
  /// 列举 store root 下的远端对象（Cloud 模式）。
  ///
  /// - `tbl`：只用于路由到服务该分区的 shard；
  /// - `prefix`：**相对 store root**（`cloud_store_path` 的 `<bucket>/<prefix>`
  ///   中后一段）的路径，对齐 C++ `ListObjectRequest::SetRemotePath`
  ///   （`eloqstore/src/storage/object_store.cpp` 的 `ComposeKeyFromRemote`）：
  ///   内部拼成 `<store root>/<prefix>` 后下推，空串表示列举 store root；
  ///   前导 `/` 忽略；不做表目录自动拼接。注意 store root 为空（未配置
  ///   `cloud_store_path` 的 prefix）时空串才等价整个 bucket；
  /// - 返回的 [`ObjectMeta::key`] 是**相对 `prefix` 的名字**（对齐 C++
  ///   `StripPrefixFromKey` / `AppendParsedEntry`），不是完整 bucket key；
  /// - 响应字段对应 C++ `ListObjectRequest::SetDetails` 的
  ///   `utils::CloudObjectInfo`：`key` ↔ `name`、`size`、`etag`（fkv 扩展）、
  ///   `last_modified` ↔ `mod_time`（S3 `LastModified`）。`path` 可由
  ///   `<store root>/<prefix>/<key>` 重建；`is_dir` 用非递归列举的 common
  ///   prefix 合成条目表达（`key` 以 `/` 结尾 + 空 `etag` + `size == 0`），
  ///   真实对象名也可能以 `/` 结尾，需要严格区分时只能依赖列举来源语义。
  ///   详见 [`ObjectMeta`]；
  /// - `token` / `max_keys`：分页契约同 `fkv_object::ObjectStore::list_page`，
  ///   `next_token` 对调用方不透明（实现上是上页最后一个后端绝对 key），只能
  ///   原样回传；**不能与 C++ 的 provider continuation token 互换**（形态
  ///   不同，跨实现复用会列错位置）；`max_keys == 0` 表示不限数量；
  /// - `recursive`：对齐 C++ `ListObjectRequest::SetRecursive`（默认 `false`）。
  ///   `false` 只列 `prefix` 的当前层对象与 common prefixes（以 `/` 结尾的合成
  ///   条目，`etag` 为空、`size == 0`、`last_modified` 为 `None`），等价 S3
  ///   ListObjectsV2 带 `delimiter=/`；`true` 递归列举全部后代
  ///   （`fkv_object::ObjectStore::list_page` 语义）。
  ///
  /// 仅 `Backend::Cloud` 支持；其它后端没有对象存储，返回 `InvalidArgs`（空列表
  /// 会被误解为「远端确实没有对象」）。
  ListObject {
    tbl: TableIdent,
    prefix: String,
    token: Option<String>,
    max_keys: usize,
    recursive: bool,
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

/// 请求的路由类别：shard 只按它选择准入 / 执行通道，不再各自维护变体清单。
///
/// 由 [`request_meta!`] 的变体表生成 [`Request::kind`]：新增 `Request` 变体
/// 不在表中归类即编译失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
  /// 只读请求：只借 `&Store`，可提交为独立 compio task 并发推进（eng-32b）。
  ///
  /// `ListArchiveTags` / `ListObject` 同样只读（列 manifest 名 / 远端对象），
  /// 不需要 `&mut Store`，可与普通读并发。
  Read,
  /// 写类请求：进 per-partition FIFO 写队列，同一分区严格串行（对齐 C++
  /// `PendingWriteQueue`）。
  ///
  /// `Reopen` / `AttachShard` / `DetachShard` / `Shutdown` 是控制请求，不进队列：
  /// `Reopen` 有独立的延迟重试队列，其余必须旁路，否则会与排空逻辑互等。
  /// `Prewarm` 会写本地 cache 文件（与同分区写路径可能争用文件名），因此也进
  /// per-partition 队列串行化；它不需要 `&mut Store`，队列只提供顺序与并发门。
  Write,
  /// 需要独占 `&mut Store`、但不进 per-partition 写队列的串行数据面请求。
  ///
  /// `Reopen` / `ReopenExternal` 自己管理 manifest 与延迟重试。`GetLarge`
  /// 不是 `Serial`：它只借 `&Store`，fixed buffer 由读侧单飞借出（见
  /// `request_meta!` 变体表）。
  Serial,
  /// 控制面请求：携带的是 shard 身份，而不是某个分区。
  ///
  /// 这类请求必须经 `Runtime::attach_shard` / `detach_shard` / `shutdown` 发出，
  /// 它们负责记录 attach 身份与清视图；走按 `TableIdent` 路由的数据面入口会
  /// 绕过这些收尾，并按调用方的 `tbl` 清错 shard。
  ///
  /// `ListStandbyPartition` 没有 `tbl`（[`Request::table`] 回 `None`）：按调用方
  /// `tbl` 路由会打到任意 shard，而该请求固定由 shard 0 的 standby service 应答。
  Control,
}

/// 请求入队优先级：前台读 / 写 / 控制为高，后台维护为低（对齐 C++
/// `ready_tasks_` / `low_priority_ready_tasks_`）。
///
/// 注意：不把写请求静态降为低优先。C++ 是写任务在 `Apply` / `WritePage` 后
/// 主动 `YieldToLowPQ`，入队即低会破坏 eng-33 的每分区写顺序与前台写延迟。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPriority {
  High,
  Low,
}

/// `RequestMeta::flags` 的位标记：逐变体列举时用 `|` 组合，避免为每个布尔
/// 维度各写一份 match。
const AUTO_REOPEN: u16 = 1 << 0;
const SKIPS_OPEN: u16 = 1 << 1;
const ALLOW_MISSING: u16 = 1 << 2;
const ADMIT_BYPASS: u16 = 1 << 3;
const OPENS_MANIFEST: u16 = 1 << 4;
const ACTIVE_REOPEN: u16 = 1 << 5;
const WRITES_DATA: u16 = 1 << 6;

/// 单个 `Request` 变体的分类属性（编译期常量表，无运行时状态）。
#[derive(Clone, Copy)]
struct RequestMeta {
  kind: RequestKind,
  pri: RequestPriority,
  flags: u16,
}

/// `Request` 分类属性的唯一列举处。
///
/// 每个变体只在这里出现一次，同时生成 [`Request::kind`] / [`Request::table`]
/// 与 shard 策略查询。生成的方法都是穷举 match：新增 `Request` 变体而漏写本
/// 表时直接编译失败，不会静默落入某个默认分类。
///
/// 三组只区分变体形状：`table` 带 `tbl` 字段，`fields` 有其它字段，`unit` 是
/// unit variant；用于生成 `table()` 与 match 模式。
macro_rules! request_meta {
  (
    table { $($t_variant:ident => $t_meta:tt),* $(,)? }
    fields { $($f_variant:ident => $f_meta:tt),* $(,)? }
    unit { $($u_variant:ident => $u_meta:tt),* $(,)? }
  ) => {
    impl Request {
      /// 本变体的分类属性。
      fn meta(&self) -> RequestMeta {
        match self {
          $(Self::$t_variant { .. } => request_meta!(@meta $t_meta),)*
          $(Self::$f_variant { .. } => request_meta!(@meta $f_meta),)*
          $(Self::$u_variant => request_meta!(@meta $u_meta),)*
        }
      }

      /// 路由类别（见 [`RequestKind`]）。
      pub fn kind(&self) -> RequestKind {
        self.meta().kind
      }

      /// 入队优先级。
      pub fn priority(&self) -> RequestPriority {
        self.meta().pri
      }

      /// 请求携带的分区；控制面请求（attach / detach / 列举 standby 分区 /
      /// shutdown）没有分区，返回 `None`。
      pub fn table(&self) -> Option<&TableIdent> {
        match self {
          $(Self::$t_variant { tbl, .. } => Some(tbl),)*
          $(Self::$f_variant { .. } => None,)*
          $(Self::$u_variant => None,)*
        }
      }

      /// 控制面请求：见 [`RequestKind::Control`]。
      pub fn is_control_plane(&self) -> bool {
        self.kind() == RequestKind::Control
      }

      /// `ResourceMissing` 时挂等待 manifest 的读请求。对齐 C++ `AutoReopenRetry`：
      /// 只有读请求（`Get` / `GetLarge` / `Floor` / `Scan`）自动重排；写请求
      /// 直接失败，由调用方决定是否重试。
      pub fn auto_reopen_read(&self) -> bool {
        self.meta().flags & AUTO_REOPEN != 0
      }

      /// 不依赖「表已打开」的控制面请求。
      ///
      /// 这些请求只碰 manifest 文件名 / 远端对象 / 本地 cache，不应为了执行它们
      /// 而触发本地空表懒创建或等待 manifest 可见：
      /// - `ReopenArchive` / `DeleteArchive`：归档文件读写；
      /// - `Drop`：删表不依赖打开状态；
      /// - `Prewarm`：只拉远端文件到本地 cache；
      /// - `ListArchiveTags`：只列 manifest 名；
      /// - `ListObject`：只列远端对象。
      ///
      /// 读窗口的准入（`window_admit`）与读任务提交（`submit_read_li`）也复用它，
      /// 否则 `ListArchiveTags` / `ListObject` 会先 `ensure_table_ready` 再执行，
      /// 在 Cloud 上无 manifest 的分区会被误判为 `ResourceMissing`。
      pub fn skips_table_open(&self) -> bool {
        self.meta().flags & SKIPS_OPEN != 0
      }

      /// 空分区上 `ResourceMissing` 不算失败：`CreateArchive` / `CreateBranch` /
      /// `DeleteBranch` 对「尚无 manifest 的空分区」是对齐 C++ 的成功无操作。
      pub fn allow_missing(&self) -> bool {
        self.meta().flags & ALLOW_MISSING != 0
      }

      /// 准入阶段即使 shard `ExpiredTerm` 也必须放行（attach / detach /
      /// shutdown）：对齐 C++ `AdmitRequest`，否则过期 shard 无法被切走。
      pub fn admit_bypass(&self) -> bool {
        self.meta().flags & ADMIT_BYPASS != 0
      }

      /// 自己执行 auto_reopen：`Reopen` / `ReopenExternal` 在 main branch 下
      /// 无需先 `ensure_table_ready`，否则首次访问同一 manifest 会读两次。
      pub fn opens_manifest(&self) -> bool {
        self.meta().flags & OPENS_MANIFEST != 0
      }

      /// 外部 `Reopen`（不建 driver、立即回 `ResourceMissing`）：对齐 C++
      /// 外部 `ReopenRequest` 一次即回，不在 fkv 内挂满 `retry_times ×
      /// pending_time`。
      pub fn active_reopen(&self) -> bool {
        self.meta().flags & ACTIVE_REOPEN != 0
      }

      /// 写数据请求（`Put` / `PutLarge`）：成功后需要登记写后维护（compact /
      /// GC / TTL 清理）。
      pub fn writes_data(&self) -> bool {
        self.meta().flags & WRITES_DATA != 0
      }
    }
  };
  (@meta { kind: $kind:ident, pri: $pri:ident, flags: $flags:expr }) => {
    RequestMeta {
      kind: RequestKind::$kind,
      pri: RequestPriority::$pri,
      flags: $flags,
    }
  };
}

request_meta! {
  table {
    // 写数据：成功后按放大率登记 compact / GC / TTL 维护。
    Put => { kind: Write, pri: High, flags: WRITES_DATA },
    Get => { kind: Read, pri: High, flags: AUTO_REOPEN },
    PutLarge => { kind: Write, pri: High, flags: WRITES_DATA },
    // 大值读只借 `&Store`（`get_large_into` 收 `&self`）：作为读任务并发推进，
    // fixed buffer 独占由读侧单飞约束保证（io-05）。
    GetLarge => { kind: Read, pri: High, flags: AUTO_REOPEN },
    Scan => { kind: Read, pri: High, flags: AUTO_REOPEN },
    // 自己 reopen：main branch 下不先 ensure，否则同一 manifest 读两次。
    Reopen => {
      kind: Serial,
      pri: High,
      flags: OPENS_MANIFEST | ACTIVE_REOPEN
    },
    ReopenExternal => { kind: Serial, pri: High, flags: OPENS_MANIFEST },
    FileGc => { kind: Write, pri: Low, flags: 0 },
    LocalGc => { kind: Write, pri: Low, flags: 0 },
    Compact => { kind: Write, pri: Low, flags: 0 },
    Truncate => { kind: Write, pri: High, flags: 0 },
    Floor => { kind: Read, pri: High, flags: AUTO_REOPEN },
    // 删表不依赖表已打开：open_empty 失败 / 损坏 manifest 的分区也要能删。
    Drop => { kind: Write, pri: High, flags: SKIPS_OPEN },
    CleanExpired => { kind: Write, pri: Low, flags: 0 },
    // 空分区（尚无 manifest）上是对齐 C++ 的成功无操作。
    CreateArchive => { kind: Write, pri: Low, flags: ALLOW_MISSING },
    // 归档重开不经过 ensure_table_ready，避免为它懒建空表。
    ReopenArchive => { kind: Write, pri: High, flags: SKIPS_OPEN },
    DeleteArchive => { kind: Write, pri: Low, flags: SKIPS_OPEN },
    // 预热写本地 cache（与同分区写路径可能争用文件名）：进写队列串行化，
    // 但不需要 `&mut Store`，低优先避免占住前台通道。
    Prewarm => { kind: Write, pri: Low, flags: SKIPS_OPEN },
    // 只列 manifest 名，开销小且调用方期望快速回执：高优先。
    ListArchiveTags => { kind: Read, pri: High, flags: SKIPS_OPEN },
    // 可能扫大 prefix：低优先，避免长时间占用高优先通道。
    ListObject => { kind: Read, pri: Low, flags: SKIPS_OPEN },
    CreateBranch => { kind: Write, pri: High, flags: ALLOW_MISSING },
    DeleteBranch => { kind: Write, pri: Low, flags: ALLOW_MISSING },
  }
  fields {
    // 控制面身份切换：ExpiredTerm 时仍必须放行，否则过期 shard 无法被切走。
    AttachShard => { kind: Control, pri: High, flags: ADMIT_BYPASS },
    DetachShard => { kind: Control, pri: High, flags: ADMIT_BYPASS },
  }
  unit {
    // 只列 standby master 上的分区：控制面观察请求，开销小，保持高优先；
    // 但仍受 attach 状态约束（不参与 admit 旁路）。
    ListStandbyPartition => { kind: Control, pri: High, flags: 0 },
    Shutdown => { kind: Control, pri: High, flags: ADMIT_BYPASS },
  }
}

impl Request {
  pub fn is_shutdown(&self) -> bool {
    matches!(self, Self::Shutdown)
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

/// 扫描回执：只有 `(key, value)` 与续页标记。
///
/// C++ `ScanRequest::Entries()` 的 `KvEntry` 还带 `timestamp_` /
/// `expire_ts_`；需要这两个字段时用 `fkv_store::Store::scan_entry*`（Local
/// SDK 对应 `fkv_sdk::KvStore::scan_kv*`），返回 `KvEntry`。
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
///
/// 对应 C++ `utils::CloudObjectInfo` 的字段：
/// - `key` ↔ `name`（相对 `ListObject::prefix` 的名字）；
/// - `etag` / `size` 同名字段（C++ `CloudObjectInfo` 没有 `etag`，属无害扩展）；
/// - `last_modified` ↔ `mod_time`（S3 `LastModified`）；C++ 保留原始时间字符串，
///   fkv 保留类型化时间点；
/// - `path` ↔ C++ `path`：不单独返回，可由
///   `<store root>/<prefix>/<key>` 重建（`ListObject::prefix` 与 store root
///   都已知）；
/// - `is_dir` ↔ C++ `is_dir`：不单独返回。非递归列举的 common prefix 是
///   合成条目（`key` 以 `/` 结尾、`etag` 为空、`size == 0`），真实对象三者
///   不会同时成立；但真实对象名也可以 `/` 结尾，需要严格区分时只能依赖
///   后端列举语义（C++ `is_dir` 由 `Contents` / `CommonPrefixes` 来源决定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
  pub key: String,
  pub etag: String,
  pub size: u64,
  /// 最后修改时间（S3 `LastModified`），对齐 C++ `CloudObjectInfo::mod_time`。
  ///
  /// `None` 表示该来源不提供：非递归列举的 common prefix 合成条目，以及
  /// 不记录修改时间的后端（`MemObjectStore`）。
  pub last_modified: Option<SystemTime>,
}

/// 一页远端对象列举结果。
///
/// 契约同 `fkv_object::ObjectStore::list_page`：`meta_li` 按 key 字节序升序。
///
/// `Request::ListObject` 下 `meta_li[].key` 是相对 `Request::ListObject::prefix`
/// 的名字（对齐 C++ `StripPrefixFromKey`，不是完整 bucket key）；`next_token`
/// 是实现内部的后端绝对 key——对调用方**不透明**，只能原样回传给下一次
/// `ListObject`。**不要与 C++ 的 token 混用**：C++ `ListObjectRequest` 的
/// continuation token 是 provider token（C++ `object_store.cpp` /
/// `cloud_backend.cpp`），fkv 的 token 是「上一页最后一个后端绝对 key」；
/// 两者都只能在本实现内回传，禁止跨实现复用或逐项对拍。`None` 表示已到
/// 末尾。非递归列举下，common prefixes 以 `/` 结尾的合成条目出现
/// （`etag` 为空、`size == 0`、`last_modified` 为 `None`），排序与真实对象合并。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPage {
  pub meta_li: Vec<ObjectMeta>,
  pub next_token: Option<String>,
}

#[derive(Debug)]
pub enum Response {
  Put,
  /// 点读结果：仅完整 value（`None` = 未命中 / 已过期）。
  ///
  /// C++ `ReadRequest` 还输出 `ts_` / `expire_ts_`，large entry 默认返回
  /// metadata；进程内需要这些字段时用 `fkv_store::Store::get_entry` /
  /// `fkv_sdk::KvStore::get_entry`。
  Get(Option<Vec<u8>>),
  /// 大 value 读取结果：完整 value + metadata trailer。
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
