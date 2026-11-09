//! 表级内存状态：table lifecycle、页缓存、字典、快照与 mapper 访问。

use std::cell::RefCell;

use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_mapping::{MappingSnapshot, PageMapper, SnapshotSource};
use fkv_types::{
  BranchFileRange, DEFAULT_SEGMENTS_PER_FILE_SHIFT, FilePageId, NULL_PAGE, PageId, TableIdent,
};

use crate::{PageCacheStat, Store, TableState, page_cache::PageCache, table_state_bytes};

impl<I: PageIo> Store<I> {
  /// 所有表 root 状态的近似常驻字节（诊断 / 测试；按当前状态重新汇总）。
  pub fn root_meta_bytes(&self) -> usize {
    self.table_map.values().map(table_state_bytes).sum()
  }

  /// 驱逐某 table 的内存状态（root / mapper / segment / page cache / 字典 /
  /// manifest 元数据），并清 IO 层缓存。detach 或重新 attach 前调用；
  /// 之后访问该表会走 reopen 重新加载。
  pub fn evict_table(&mut self, tbl: &TableIdent) {
    if let Some(state) = self.table_map.remove(tbl) {
      self.root_meta_used = self.root_meta_used.saturating_sub(state.bytes);
    }
    self.touch_seq.remove(tbl);
    self.io.evict_table(tbl);
  }

  /// 表是否已加载到内存（含懒创建的空表）。淘汰 / drop 后立即为 false。
  pub fn is_open(&self, tbl: &TableIdent) -> bool {
    self
      .table_map
      .get(tbl)
      .is_some_and(|state| state.root.is_some())
  }

  /// 已打开表的快照（测试 / 诊断用）：detach 驱逐由 shard 自持分区清单完成，
  /// 不经过本方法。
  pub fn open_table_li(&self) -> Vec<TableIdent> {
    self
      .table_map
      .iter()
      .filter(|(_, state)| state.root.is_some())
      .map(|(tbl, _)| tbl.clone())
      .collect()
  }

  /// 借出（必要时创建）该表的状态槽；只登记槽位，不改变 `root`（打开语义）。
  pub(crate) fn table_state(&mut self, tbl: &TableIdent) -> &mut TableState {
    self.table_map.entry(tbl.clone()).or_default()
  }

  /// 懒创建空表：无 manifest 的首次访问按空分区处理，避免每次请求重复 reopen。
  pub fn open_empty(&mut self, tbl: &TableIdent) {
    if self.is_open(tbl) {
      return;
    }
    let source = self.generation_source.clone();
    let alloc_mode = self.alloc_mode;
    let snapshot_source = self.snapshots.generation_source();
    let state = self.table_state(tbl);
    state.mapper.get_or_insert_with(|| {
      let mut mapper = PageMapper::new_with_source(source, alloc_mode, 0);
      mapper.set_snapshot_generation_source(snapshot_source);
      mapper
    });
    state.root = Some(NULL_PAGE);
    state.ttl_root = NULL_PAGE;
    self.ensure_page_cache(tbl);
    self.touch_table(tbl);
  }

  /// 记录 table 最近一次写 / 打开，并按 `root_meta_cache_size` 字节预算驱逐。
  ///
  /// 对齐 C++ `RootMetaMgr`：只有字节预算一条上限（没有「打开表个数」上限）。
  /// 触摸与逐出均摊 O(1)，陈旧项惰性跳过；预算为 0 时直接返回（默认路径零开销）。
  /// 字节记账在触摸时刷新当前表，写批次收尾再补一次（[`Self::refresh_table_bytes`]）。
  pub(crate) fn touch_table(&mut self, tbl: &TableIdent) {
    if self.root_meta_cache_bytes == 0 {
      return;
    }
    self.refresh_table_bytes(tbl);
    self.touch_clock += 1;
    let seq = self.touch_clock;
    self.touch_seq.insert(tbl.clone(), seq);
    self.table_order_li.push_back((tbl.clone(), seq));
    // 陈旧项最多占一半，队列长度保持 O(有效表数)。
    let queue_limit = self.touch_seq.len().saturating_mul(2) + 64;
    if self.table_order_li.len() > queue_limit {
      self
        .table_order_li
        .retain(|(item, item_seq)| self.touch_seq.get(item) == Some(item_seq));
    }
    self.evict_over_root_meta(tbl);
  }

  /// 按当前状态重算该表的近似字节并更新合计。
  pub(crate) fn refresh_table_bytes(&mut self, tbl: &TableIdent) {
    let bytes = self.table_map.get(tbl).map_or(0, table_state_bytes);
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    self.root_meta_used = self
      .root_meta_used
      .saturating_sub(state.bytes)
      .saturating_add(bytes);
    state.bytes = bytes;
  }

  /// 超出 `root_meta_cache_bytes` 时从 LRU 尾驱逐整表状态。
  ///
  /// 保留当前正在写 / 打开的表；跳过仍被读快照引用（快照独占的 mapping 不会
  /// 因驱逐释放，且 reopen 会打断请求）或字典尚未随 manifest 持久化的表。
  /// 单轮最多扫描一次有效表数：候选全部被保留时接受超预算（例如单表大于预算），
  /// 不旋转队列死循环。
  pub(crate) fn evict_over_root_meta(&mut self, keep: &TableIdent) {
    if self.root_meta_cache_bytes == 0 || self.root_meta_used <= self.root_meta_cache_bytes {
      return;
    }
    let mut scanned = self.touch_seq.len();
    while self.root_meta_used > self.root_meta_cache_bytes && scanned > 0 {
      scanned -= 1;
      let Some((oldest, oldest_seq)) = self.table_order_li.pop_front() else {
        break;
      };
      if self.touch_seq.get(&oldest) != Some(&oldest_seq) {
        continue;
      }
      if &oldest == keep || !self.root_meta_evictable(&oldest) {
        self.table_order_li.push_back((oldest, oldest_seq));
        continue;
      }
      self.evict_table(&oldest);
    }
  }

  /// 表是否可被字节预算驱逐：无活跃读快照，且字典没有未持久化的训练结果。
  fn root_meta_evictable(&self, tbl: &TableIdent) -> bool {
    self.snapshots.live_num(tbl) == 0
      && self.table_map.get(tbl).is_none_or(|state| {
        !state
          .compression
          .as_ref()
          .is_some_and(DictCompression::dirty)
      })
  }

  /// 该表当前字典字节；未启用压缩或无字典返回空。
  fn dict_bytes(&self, tbl: &TableIdent) -> &[u8] {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.compression.as_ref())
      .map_or(&[], DictCompression::dictionary_bytes)
  }

  /// 字典已随 snapshot 持久化，清除 dirty 标记（对齐 C++ `ClearDirty`）。
  pub(crate) fn clear_dict_dirty(&mut self, tbl: &TableIdent) {
    if let Some(compression) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.compression.as_mut())
    {
      compression.clear_dirty();
    }
  }

  /// 为表装载已有 zstd 字典；启用压缩（[`Store::with_compression_level`]）时
  /// 该表后续写入按字典压缩，manifest 会持久化字典字节，reopen 时自动恢复。
  ///
  /// 装载成功即标记字典待持久化（`mark_dirty`）：manifest 已建的表下一次
  /// flush 走全量 snapshot 携带 `dict_bytes`——增量 record 不含字典，不置脏
  /// 的话 reopen 后 `has_dictionary == false`，已写入的 Dictionary 压缩页
  /// 永久 `Corrupted`（sem-67 声明 1）。
  ///
  /// 该表已有字典（人工注入 / 自动训练 / replay 恢复）时返回
  /// `Err(KvError::InvalidArgs)`，不替换：
  /// zstd 帧不内嵌字典、也不校验字典身份，静默换字典会让旧 Dictionary 压缩页
  /// 按新字典解出**错值**（sem-67 声明 2）。对齐 C++ 封闭生命周期——
  /// `LoadDictionary` 只在 replay 内部调用、训练有 `HasDictionary()` 门，
  /// 不存在运行中注入 / 换字典的状态。
  ///
  /// 未启用压缩时字典仍可用于解码历史压缩页，但新写入保持 raw（对齐 C++
  /// `enable_compression` 门控）。
  pub fn set_dictionary(&mut self, tbl: &TableIdent, dict: Vec<u8>) -> Result<()> {
    if self.has_dictionary(tbl) {
      // 拒绝替换活字典：旧 Dictionary 压缩页依赖原字典内容解压，替换后解码
      // 静默失真（无任何错误信号），只能整体拒绝。
      return Err(KvError::InvalidArgs);
    }
    let level = self.compression_level.unwrap_or_default();
    let state = self.table_state(tbl);
    // get_or_insert：装载失败（字典非法）时保留原压缩状态（含已积累样本），
    // 不像旧 take + ? 路径把整份状态静默丢掉。
    let compression = state
      .compression
      .get_or_insert_with(|| DictCompression::new(level));
    compression.load_dictionary(dict)?;
    // 注入的字典尚未进入 manifest：显式置脏，下一次 flush 走全量 snapshot
    // 持久化 dict_bytes。不得让 `load_dictionary` 本身置脏——manifest replay
    // 复用它装载已在盘上的字典，置脏会让每次 reopen 后首个 flush 被迫全量
    // snapshot（对齐 C++ `LoadDictionary` 不置脏）。
    compression.mark_dirty();
    self.refresh_table_bytes(tbl);
    Ok(())
  }

  /// 该表是否已装载字典（启用压缩时写入会走 Dictionary 压缩）。
  pub fn has_dictionary(&self, tbl: &TableIdent) -> bool {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.compression.as_ref())
      .is_some_and(DictCompression::has_dictionary)
  }

  /// 写批次出口：消费本批的变更记录（不变量检查见方法体）。
  ///
  /// 未变更子树的索引页保持有效（对齐 C++ 只替换变更的 `MemIndexPage`）。
  /// core-02 之后逻辑 PageId 可复用（空闲链表），失效依赖 `free_page` 把释放的
  /// PageId 记入 `changed_page_li`：复用分配前旧缓存条目已在本函数中移除。
  pub(crate) fn consume_changed_pages(&mut self, tbl: &TableIdent) {
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    let _ = state
      .mapper
      .as_mut()
      .map(|mapper| mapper.take_changed_page_li())
      .unwrap_or_default();
    if let Some(segment_mapper) = state.segment_mapper.as_mut() {
      let _ = segment_mapper.take_changed_page_li();
    }
    // 批出口不变量：变更记录必须已被消费，禁止跨批次累积（eng-68）。
    debug_assert!(
      state
        .mapper
        .as_ref()
        .is_none_or(|mapper| mapper.changed_page_li().is_empty()),
      "data changed_page_li 未在批出口清空"
    );
    debug_assert!(
      state
        .segment_mapper
        .as_ref()
        .is_none_or(|mapper| mapper.changed_page_li().is_empty()),
      "segment changed_page_li 未在批出口清空"
    );
  }

  /// 确保该表有页缓存（读写共用）；预算 `0`（显式关闭）时不建，两侧直连 IO。
  ///
  /// 表打开（`open_empty` / `reopen`）与首次写入都会调用：读路径只有 `&self`
  /// 无法懒建实例，只读打开的 store（`--read-only-existing`）必须在这里建好。
  pub(crate) fn ensure_page_cache(&mut self, tbl: &TableIdent) {
    let capacity = self.page_cache_bytes;
    if capacity == 0 {
      return;
    }
    let state = self.table_state(tbl);
    state
      .page_cache
      .get_or_insert_with(|| RefCell::new(PageCache::new(capacity)));
  }

  /// 该表的页缓存句柄；未建（预算 `0` / 表未打开）时为 `None`。
  pub(crate) fn page_cache(&self, tbl: &TableIdent) -> Option<&RefCell<PageCache>> {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.page_cache.as_ref())
  }

  /// 消费 mapper 记录的已释放物理页，逐条失效页缓存（批次结算的收尾动作）。
  ///
  /// 这是「已释放的物理页不可能命中缓存」的唯一执行点：Pooled 分配器会把这些
  /// id 复用给新页，失效后即便复用页在被写出之前被读到，也只能回源磁盘。
  pub(crate) fn invalidate_freed_page_cache(&mut self, tbl: &TableIdent) {
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    let Some(mapper) = state.mapper.as_mut() else {
      return;
    };
    let freed_li = mapper.take_freed_file_page_li();
    if freed_li.is_empty() {
      return;
    }
    let Some(cache) = state.page_cache.as_ref() else {
      return;
    };
    let mut cache = cache.borrow_mut();
    for file_page_id in freed_li {
      cache.remove(file_page_id);
    }
  }

  /// 全量清空该表页缓存（compaction / reopen / 截断 / 批次回滚）。
  ///
  /// 这些路径要么走裸 IO 直接改写物理页（compaction），要么整体替换 / 回退
  /// 映射（reopen / truncate / abort），逐页跟踪得不偿失；清空后第一批按
  /// 未命中重新回填，正确性不依赖残留条目。
  pub(crate) fn clear_page_cache(&self, tbl: &TableIdent) {
    let Some(state) = self.table_map.get(tbl) else {
      return;
    };
    if let Some(cache) = state.page_cache.as_ref() {
      cache.borrow_mut().clear();
    }
  }

  /// 外部工具 / 测试直接改写 data 物理页后主动失效全部表的页缓存。
  ///
  /// 页缓存的硬不变量（见 [`crate::page_cache`]）只在「物理页只被读写装饰器
  /// 与自身已清空缓存的 compaction 改写」时成立。绕过 `Store` 直接对
  /// [`Self::page_io`] 落盘的旁路写打破了该前提（同一 `FilePageId` 的盘上字节
  /// 已变而缓存不知），必须显式调用本方法，否则下一次读写会命中陈旧整页。
  /// 与 [`Self::invalidate_path_cache`] 同定位：外部改动后由调用方告知。
  pub fn invalidate_page_cache(&self) {
    for state in self.table_map.values() {
      if let Some(cache) = state.page_cache.as_ref() {
        cache.borrow_mut().clear();
      }
    }
  }

  /// 页缓存的累计命中 / 未命中 / 逐出与当前占用；无缓存时全零。
  pub fn page_cache_stat(&self) -> PageCacheStat {
    let mut stat = PageCacheStat::default();
    for state in self.table_map.values() {
      let Some(cache) = state.page_cache.as_ref() else {
        continue;
      };
      let cache = cache.borrow().stat();
      stat.hit += cache.hit;
      stat.miss += cache.miss;
      stat.evict += cache.evict;
      stat.bytes += cache.bytes;
      stat.len += cache.len;
      stat.capacity = stat.capacity.max(cache.capacity);
    }
    stat
  }

  /// 原地刷新该表的 branch 元数据：以恢复出来的元数据为基线，保留父分支 /
  /// 旧 term 的 `file_range_li` 历史链，末尾同 (branch, term) 区间原地合并。
  pub(crate) fn refresh_branch_metadata(&mut self, tbl: &TableIdent) {
    // 热路径（同 branch / term 的连续写）只需要原地更新末尾区间，不能先克隆
    // branch 名：单条写每次都会走到这里，两次 `String` 分配是纯粹的固定成本。
    let branch_name = self.branch_name.as_str();
    let term = self.term;
    let page_shift = self.pages_per_file_shift;
    let segment_shift = self.segment_shift();
    let Some(state) = self.table_map.get_mut(tbl) else {
      return;
    };
    if state.mapper.is_none() && state.segment_mapper.is_none() {
      return;
    }
    let max_file_id = state
      .mapper
      .as_ref()
      .map_or(0, |mapper| mapper.max_fp_id() >> page_shift);
    let max_segment_file_id = state
      .segment_mapper
      .as_ref()
      .map_or(0, |mapper| mapper.max_fp_id() >> segment_shift);
    // 只在写 manifest 时借出引用，不再整条克隆回写。
    let metadata = state
      .branch_metadata
      .get_or_insert_with(|| BranchManifestMetadata::new(branch_name, term));
    match metadata.file_range_li.last_mut() {
      Some(last) if last.branch_name.as_ref() == branch_name && last.term == term => {
        last.max_file_id = last.max_file_id.max(max_file_id);
        last.max_segment_file_id = last.max_segment_file_id.max(max_segment_file_id);
      }
      _ => metadata.file_range_li.push(BranchFileRange {
        branch_name: branch_name.into(),
        term,
        max_file_id,
        max_segment_file_id,
      }),
    }
  }

  /// segment 文件的 file id 位移；未启用大 value 时用后端默认槽位数。
  pub(crate) fn segment_shift(&self) -> u8 {
    self
      .large_value_layout
      .map_or(DEFAULT_SEGMENTS_PER_FILE_SHIFT, |layout| {
        layout.segments_per_file_shift()
      })
  }

  /// 构造 snapshot record。
  ///
  /// segment 段是可选段：只有 `segment_mapping != nullptr` 时才追加
  /// `max_segment_fp_id(varint64) | seg_len(4B) | seg varints`，否则整段省略。
  /// 因此本函数是否写 segment 段只由「该表是否有活跃 segment mapping」决定：
  /// 空的 `PageMapper`（未写过 large value）不算活跃，避免 branch/archive
  /// manifest 多出一段空 segment。
  ///
  /// `max_segment_fp_id` 只在存在活跃 segment mapping 时生效（branch 用它写
  /// 预留给子分支的 segment 起点）。
  pub(crate) fn snapshot_record(
    &self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
    max_fp_id: Option<FilePageId>,
    max_segment_fp_id: Option<FilePageId>,
    branch_override: Option<&BranchManifestMetadata>,
  ) -> Result<Vec<u8>> {
    let state = self.table_map.get(tbl);
    let mapper = state.and_then(|state| state.mapper.as_ref());
    // 与 C++ `GetSegmentMapper`（replayer.cpp）一致：空 mapping 且高水位为 0
    // 等价于「没有 segment mapper」。`put_large` 会为未命中阈值的批次建出
    // 空 mapper，这类 mapper 不应写进 manifest。
    let segment_mapper = state
      .and_then(|state| state.segment_mapper.as_ref())
      .filter(|mapper| !mapper.tbl().is_empty() || mapper.max_fp_id() > 0);
    let default_metadata: BranchManifestMetadata;
    let branch_metadata = match branch_override {
      Some(branch) => branch,
      None => match state.and_then(|state| state.branch_metadata.as_ref()) {
        Some(branch) => branch,
        None => {
          default_metadata = BranchManifestMetadata::new(self.branch_name.clone(), self.term);
          &default_metadata
        }
      },
    };
    // 直接流式编码 mapping，跳过整表摊平的临时 `Vec<u64>`（eng-76）。
    //
    // 段水位纵深防御（pooled-12）：活跃 manifest / 归档快照（`branch_override`
    // 为空）的 `max_segment_fp_id` 不得低于当前 segment mapper——任何回滚 /
    // 外部快照安装路径都不能把盘上段水位写低，否则崩溃重开后的新大 value
    // 仍会从低水位覆盖旧段页。分支快照（`branch_override` 非空）例外：传入值
    // 描述的是**新子分支**的段起点，不能并入父分支已预留的段水位。
    let current_seg_max = segment_mapper.map_or(0, PageMapper::max_fp_id);
    let max_segment_fp_id = match (max_segment_fp_id, branch_override) {
      (Some(explicit), Some(_)) => explicit,
      (explicit, None) => explicit.unwrap_or(current_seg_max).max(current_seg_max),
      (None, Some(_)) => current_seg_max,
    };
    ManifestBuilder::new().snapshot_with(
      SnapshotArg {
        root_id,
        ttl_root: ttl_root_id,
        max_fp_id: max_fp_id.unwrap_or_else(|| mapper.map_or(0, PageMapper::max_fp_id)),
        dict_bytes: self.dict_bytes(tbl),
        branch_metadata,
        max_segment_fp_id,
      },
      mapper
        .into_iter()
        .flat_map(|mapper| mapper.tbl().iter_value_li()),
      segment_mapper.map(|mapper| mapper.tbl().iter_value_li()),
    )
  }

  pub fn page_io(&self) -> &I {
    &self.io
  }

  pub fn mapper(&self, tbl: &TableIdent) -> Option<&PageMapper> {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.mapper.as_ref())
  }

  pub fn segment_mapper(&self, tbl: &TableIdent) -> Option<&PageMapper> {
    self
      .table_map
      .get(tbl)
      .and_then(|state| state.segment_mapper.as_ref())
  }

  /// 取一份读请求可跨 `await` 持有的 mapper 快照并登记到 GC 保留表。
  ///
  /// 快照固定调用时刻的 data / segment 映射、`root_id` / `ttl_root_id` 与两侧
  /// 分配器高水位；创建代价是 O(1) 共享 chunk 指针表（不再逐 chunk 复制），
  /// 不随库容量增长。快照 `drop` 后弱引用失效，注册表在下一轮 GC 取 retained
  /// set 时惰性摘链，旧页从下一轮 GC 起可回收。
  ///
  /// 返回 `None` 表示该表尚未加载 mapper（无映射可读）。多份快照各自独立登记，
  /// 共享同一份底层 chunk 的只是数据，不是生命周期。
  ///
  /// 所有读入口（`get` / `get_large*` / `floor` / `scan*` 及写路径内部扫描）都在
  /// 请求开始时调用本方法取一次快照，之后整个请求只读该快照（27b）。
  pub fn snapshot(&self, tbl: &TableIdent) -> Option<MappingSnapshot> {
    let state = self.table_map.get(tbl)?;
    let data = state.mapper.as_ref()?;
    Some(self.snapshots.open(
      tbl,
      SnapshotSource {
        data,
        segment: state.segment_mapper.as_ref(),
        root_id: state.root.unwrap_or(NULL_PAGE),
        ttl_root_id: state.ttl_root,
      },
    ))
  }

  /// 该表当前仍被读请求持有的快照数（诊断 / 测试）。
  pub fn snapshot_num(&self, tbl: &TableIdent) -> usize {
    self.snapshots.live_num(tbl)
  }

  pub fn root_id(&self, tbl: &TableIdent) -> Option<PageId> {
    self.table_map.get(tbl).and_then(|state| state.root)
  }

  pub fn ttl_root_id(&self, tbl: &TableIdent) -> Option<PageId> {
    self.table_map.get(tbl).map(|state| state.ttl_root)
  }

  pub fn branch_file_limit(&self) -> Option<FilePageId> {
    self.branch_file_limit
  }

  pub fn branch_segment_limit(&self) -> Option<FilePageId> {
    self.branch_segment_limit
  }

  pub fn into_io(self) -> I {
    self.io
  }
}
