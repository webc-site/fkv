//! cache 状态与侵入式 LRU：节点 / 表条目 / 驻留字节 / 待删队列。

use fkv_types::{FileId, TableIdent};
use gxhash::HashMap;

/// LRU 链表空指针哨兵：节点下标 `u32::MAX` 表示无。
pub(crate) const LRU_NIL: u32 = u32::MAX;

/// 待删队列达到 `cache_budget >> EVICT_DRAIN_SHIFT`（预算的 1/8）时批量删除。
/// 即本地磁盘最多比预算多出一份「已摘链、未删除」的字节。
pub(crate) const EVICT_DRAIN_SHIFT: u32 = 3;
/// 待删队列文件数上限：文件很小时也保证批量删除的规模有界。
pub(crate) const EVICT_DRAIN_FILE_NUM: usize = 1024;
/// 待删队列驻留上限：坏盘 / 权限错误时失败项回队不能无界增长。
/// 超出后丢弃本轮淘汰项（本地文件留在盘上，由下次全量清理处理）。
pub const EVICT_PENDING_FILE_MAX: usize = 8192;
/// 连续 drain 失败达到该次数后暂停热路径重试，避免每个 IO 都撞同一个坏盘。
pub(crate) const EVICT_FAIL_PAUSE: u32 = 8;

/// 侵入式 LRU 节点：只登记**干净**（已上传）的 data / segment 文件，
/// 对齐 C++ `closed_files_` 不含 dirty 文件。`prev/next` 为节点 arena 下标。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LruNode {
  pub(crate) prev: u32,
  pub(crate) next: u32,
  /// 反查 `CacheState::table_li` 的下标，节点不复制 `TableIdent`。
  pub(crate) table_idx: u32,
  pub(crate) file_id: FileId,
  pub(crate) segment: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct FileState {
  pub(crate) size: u64,
  pub(crate) dirty: bool,
  /// 已修改但未上传的字节区间 `(offset, len)`；空表示整文件重传。
  pub(crate) dirty_range_li: Vec<(u64, u64)>,
  /// 本地内容归属的写时 `(branch, term)`：干净副本记录回源对象的 identity，
  /// 写入 / 上传后改为当前 identity。读命中前与 manifest 的 per-file 归属比对，
  /// 不一致按 cache miss 回源（io-12）。`None` 表示归属未知，按 miss 处理。
  ///
  /// 同时是 `dirty_range_li` 的归属：flush 目标的 identity 与它不一致时区间
  /// 指向旧对象，必须丢弃并改为整对象上传，否则区间写会用零前缀覆盖新对象。
  pub(crate) ident: Option<(String, u64)>,
  /// 强制整对象上传。identity 首次变更后必须置位：旧脏页属于旧对象，
  /// 新区间又归属新 identity，只写新区间会让新对象前段留下空洞。
  pub(crate) dirty_all: bool,
  /// LRU 节点下标；`LRU_NIL` 表示脏文件不在链上。
  pub(crate) lru_idx: u32,
}

/// 单表的缓存条目。data 与 segment 分开记账：`FileId` 同一编号空间，不能共用一个 map。
#[derive(Debug, Default)]
pub(crate) struct TableCache {
  /// 反查 `CacheState::table_li` 的下标。
  pub(crate) table_idx: u32,
  pub(crate) data: HashMap<FileId, FileState>,
  pub(crate) segment: HashMap<FileId, SegmentState>,
}

impl TableCache {
  /// 该表占用的驻留字节。
  pub(crate) fn bytes(&self) -> u64 {
    self.data.values().map(|entry| entry.size).sum::<u64>()
      + self.segment.values().map(|entry| entry.size).sum::<u64>()
  }

  pub(crate) fn len(&self) -> usize {
    self.data.len() + self.segment.len()
  }
}

/// 借 `tbl` 查表；只有该表首次出现时 clone 一次 `TableIdent`。
/// 同时分配 `table_li` 槽位，供 LRU 节点反查表名。
pub(crate) fn table_slot<'a>(
  table_map: &'a mut HashMap<TableIdent, TableCache>,
  table_li: &mut Vec<Option<TableIdent>>,
  free_table_li: &mut Vec<u32>,
  tbl: &TableIdent,
) -> &'a mut TableCache {
  if let Some(cache) = table_map.get_mut(tbl) {
    return cache;
  }
  let table_idx = match free_table_li.pop() {
    Some(idx) => idx,
    None => {
      table_li.push(None);
      (table_li.len() - 1) as u32
    }
  };
  table_li[table_idx as usize] = Some(tbl.clone());
  table_map.insert(
    tbl.clone(),
    TableCache {
      table_idx,
      ..TableCache::default()
    },
  );
  // 刚插入，get_mut 必中。
  table_map.get_mut(tbl).unwrap()
}

/// 归还 `table_li` 槽位；调用前表内条目必须已全部摘链。
pub(crate) fn free_table_slot(
  table_li: &mut [Option<TableIdent>],
  free_table_li: &mut Vec<u32>,
  idx: u32,
) {
  table_li[idx as usize] = None;
  free_table_li.push(idx);
}

/// 表内条目全空时清掉外层登记，避免空表常驻。
pub(crate) fn drop_if_empty(state: &mut CacheState, tbl: &TableIdent) {
  let table_idx = state
    .table_map
    .get(tbl)
    .filter(|cache| cache.data.is_empty() && cache.segment.is_empty())
    .map(|cache| cache.table_idx);
  if let Some(table_idx) = table_idx {
    state.table_map.remove(tbl);
    free_table_slot(&mut state.table_li, &mut state.free_table_li, table_idx);
  }
}

/// 从空闲链取槽位并挂到链尾（最近使用端），返回节点下标。
///
/// 空链复用 `node_li` 的空闲槽，节点 arena 上界 = 历史峰值驻留条目数。
pub(crate) fn lru_push(
  node_li: &mut Vec<LruNode>,
  free_node: &mut u32,
  head: &mut u32,
  tail: &mut u32,
  table_idx: u32,
  file_id: FileId,
  segment: bool,
) -> u32 {
  let node = LruNode {
    prev: *tail,
    next: LRU_NIL,
    table_idx,
    file_id,
    segment,
  };
  let idx = if *free_node == LRU_NIL {
    node_li.push(node);
    (node_li.len() - 1) as u32
  } else {
    let idx = *free_node;
    *free_node = node_li[idx as usize].next;
    node_li[idx as usize] = node;
    idx
  };
  match *tail {
    LRU_NIL => *head = idx,
    tail_idx => node_li[tail_idx as usize].next = idx,
  }
  *tail = idx;
  idx
}

/// 摘链并归还槽位（`next` 改作空闲链指针）。
pub(crate) fn lru_unlink(
  node_li: &mut [LruNode],
  free_node: &mut u32,
  head: &mut u32,
  tail: &mut u32,
  idx: u32,
) {
  let node = node_li[idx as usize];
  match node.prev {
    LRU_NIL => *head = node.next,
    prev => node_li[prev as usize].next = node.next,
  }
  match node.next {
    LRU_NIL => *tail = node.prev,
    next => node_li[next as usize].prev = node.prev,
  }
  node_li[idx as usize].next = *free_node;
  *free_node = idx;
}

/// 已在链上的节点挪到链尾（读命中 / 重新变干净）。
pub(crate) fn lru_move_tail(node_li: &mut [LruNode], head: &mut u32, tail: &mut u32, idx: u32) {
  if *tail == idx {
    return;
  }
  let node = node_li[idx as usize];
  match node.prev {
    LRU_NIL => *head = node.next,
    prev => node_li[prev as usize].next = node.next,
  }
  match node.next {
    LRU_NIL => *tail = node.prev,
    next => node_li[next as usize].prev = node.prev,
  }
  let tail_idx = *tail;
  node_li[idx as usize].prev = tail_idx;
  node_li[idx as usize].next = LRU_NIL;
  match tail_idx {
    LRU_NIL => *head = idx,
    t => node_li[t as usize].next = idx,
  }
  *tail = idx;
}

/// 缓存状态：一条全局侵入式 LRU 链表 + 按表嵌套的条目表。
///
/// 链表顺序即 LRU 顺序（head 最旧、tail 最新），victim 取 head 为 O(1)，
/// 不再全表扫描。对齐 C++ `lru_file_head_/tail_`。
#[derive(Debug)]
pub(crate) struct CacheState {
  pub(crate) table_map: HashMap<TableIdent, TableCache>,
  /// `table_li[table_idx]` 反查表名；`None` 表示槽位空闲。
  pub(crate) table_li: Vec<Option<TableIdent>>,
  pub(crate) free_table_li: Vec<u32>,
  pub(crate) node_li: Vec<LruNode>,
  pub(crate) free_node: u32,
  pub(crate) lru_head: u32,
  pub(crate) lru_tail: u32,
  pub(crate) resident_bytes: u64,
  /// 已摘链、待本地删除的淘汰对象；`resident_bytes` 入队时已扣减。
  pub(crate) pending_delete_li: Vec<CacheVictim>,
  pub(crate) pending_delete_bytes: u64,
  /// 入队代数。`drain_seq` 追平时说明这批淘汰已尝试过删除：失败后没有
  /// 新淘汰就不在热路径上重试，避免本地盘持续报错时每个 IO 都重试。
  pub(crate) evict_seq: u64,
  pub(crate) drain_seq: u64,
  /// 连续 drain 失败次数；达到 [`EVICT_FAIL_PAUSE`] 后暂停热路径 drain。
  pub(crate) drain_fail_streak: u32,
}

impl Default for CacheState {
  fn default() -> Self {
    Self {
      table_map: HashMap::default(),
      table_li: Vec::new(),
      free_table_li: Vec::new(),
      node_li: Vec::new(),
      free_node: LRU_NIL,
      lru_head: LRU_NIL,
      lru_tail: LRU_NIL,
      resident_bytes: 0,
      pending_delete_li: Vec::new(),
      pending_delete_bytes: 0,
      evict_seq: 0,
      drain_seq: 0,
      drain_fail_streak: 0,
    }
  }
}

/// segment 文件固定整文件上传，只记大小 / 脏标记 / LRU / 内容归属。
#[derive(Debug, Clone)]
pub(crate) struct SegmentState {
  pub(crate) size: u64,
  pub(crate) dirty: bool,
  /// 本地内容归属的写时 `(branch, term)`；语义同 [`FileState::ident`]
  /// （io-12：跨 term 重开归档时同 file id 的旧副本不能命中）。
  pub(crate) ident: Option<(String, u64)>,
  /// LRU 节点下标；`LRU_NIL` 表示脏文件不在链上。
  pub(crate) lru_idx: u32,
}

/// LRU 淘汰对象：data / segment 走不同的本地删除接口。
///
/// `bytes` 是摘链时已从 `resident_bytes` 扣减的字节数，删除失败回队时用它
/// 恢复待删字节记账。
#[derive(Debug, Clone)]
pub(crate) struct CacheVictim {
  pub(crate) tbl: TableIdent,
  pub(crate) file_id: FileId,
  pub(crate) segment: bool,
  pub(crate) bytes: u64,
}
