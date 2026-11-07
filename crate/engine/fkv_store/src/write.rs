//! 写路径：put / delete / 大 value 写入。

use std::{
  cell::RefCell,
  collections::{BTreeMap, btree_map::Entry},
};

use fkv_btree::{BatchEntry, TtlUpsertSink};
use fkv_compress::DictCompression;
use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo, ManifestIo, PageIo, SegmentIo};
use fkv_large::{
  SegmentLayout, WrittenLargeValue, write_value as write_large_value, write_value_fixed_with_len,
};
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_read::YieldBudget;
use fkv_task_write::{
  PageLayout, PreparedWriteBatch, WriteArg, is_expired, prepare_write_batch, ttl_key, write_batch,
};
use fkv_types::{NULL_PAGE, TableIdent, WriteOp, now_ms};

use crate::{
  Store,
  batch::MapperBatch,
  reborrow_segment,
  write_cache::{CachedWriteIo, WritePageCache},
};

impl<I: PageIo + ManifestIo> Store<I> {
  /// 写入口的 key 校验：非空且不超过数据页大小的一半（对齐 C++ `SetBatch`
  /// 的 `data_page_size >> 1` 约束）。整批拒绝，避免错误暴露在页构建深处。
  ///
  /// 所有公开写路径（`put` / `put_at` / `put_large*` / `put_with_term`）
  /// 都汇入 `put_at`，只在此处校验一次。
  fn check_batch_key_li(&self, batch_li: &[BatchEntry]) -> Result<()> {
    let max_len = self.page_size >> 1;
    if batch_li
      .iter()
      .any(|entry| entry.key.is_empty() || entry.key.len() > max_len)
    {
      return Err(KvError::InvalidArgs);
    }
    Ok(())
  }

  pub async fn put(&mut self, tbl: &TableIdent, batch_li: &[BatchEntry]) -> Result<()> {
    self.put_at(tbl, batch_li, now_ms()).await
  }

  /// 带时间注入的写入：对齐 C++ BatchWriteTask，`expire_ts <= now_ms` 的
  /// upsert 不写主树，只删除对应 TTL 索引项。
  ///
  /// 写入过程可回滚：中途失败（如 `OutOfMem`）时 `PageMapper::abort_batch`
  /// 恢复 mapping / 分配器，保证重试从干净状态开始（对齐 C++ `Abort`）。
  ///
  /// data mapper 与 segment mapper 同批（rev-01）：底页上的大 value 被覆盖 / 删除时
  /// 会在 `write_batch` 内释放 segment 页，失败必须一并回滚，否则 live key 指向已回收
  /// 的段，或本批新分配（`put_large`）的段泄漏。
  pub async fn put_at(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    now_ms: u64,
  ) -> Result<()> {
    self.put_at_with_ttl(tbl, batch_li, now_ms, true).await
  }

  /// 把 retired 队列中早于所有在途快照的物理页归还 Pooled 空闲池（pooled-04）。
  ///
  /// 唯一回收水位来源：[`SnapshotRegistry::min_live_generation`]——无存活快照
  /// 时为 `u64::MAX`（全部回收），有在途读时只收更老的页，因此旧快照按旧
  /// `FilePageId` 读取不会被重分配的页覆盖（pooled-01 协议）。
  ///
  /// **必须在 mapper 批次之外调用**：`abort_batch` 以 retire 队列长度为回滚点，
  /// 批次内回收会让回滚后重新活跃的页消失（见 `PageMapper::reclaim_retired`）。
  /// 写路径在开批前回收；退役页的维护路径在各自批次结算之后回收：
  /// `maintain` / `clean_expired` / `compact_file_li` / `truncate`。这些路径
  /// 同样会 retire 物理页，不能只靠下一次 `put` 才把空间还回池（否则纯维护
  /// 负载下 file page 空间只增不减）。`drop_table` 不在此列：它直接丢弃表
  /// 状态，没有可回池的 mapper 队列。
  ///
  /// 只作用于 data mapper：段 mapper 恒为 `Append`，retire 本身是 no-op。
  pub(crate) fn reclaim_retired_now(&mut self, tbl: &TableIdent) -> usize {
    // Append：`retire_file_page` 是 no-op、retired 队列恒空，直接短路。省掉
    // 每写一次的 `min_live_generation`（会 prune 注册表并升级全部弱引用）与
    // mapper 查找（perf-12）。
    if self.alloc_mode == AllocMode::Append {
      return 0;
    }
    let Some(mapper) = self
      .table_map
      .get_mut(tbl)
      .and_then(|state| state.mapper.as_mut())
    else {
      return 0;
    };
    // 队列为空时没有可回收的页：水位（快照注册表扫描）同样没有意义，
    // 只是把惰性清理延后到下一次确有退役页的写批次。
    if mapper.retired_len() == 0 {
      return 0;
    }
    mapper.reclaim_retired(self.snapshots.min_live_generation())
  }

  /// `put_at` 主体；`inline_ttl = false` 时把写后清理留给调用方在结算外层
  /// journal 之后执行（`put_large` 的段批次尚未结算，内联清理会嵌套回滚）。
  async fn put_at_with_ttl(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    now_ms: u64,
    inline_ttl: bool,
  ) -> Result<()> {
    self.check_batch_key_li(batch_li)?;
    self.touch_table(tbl);
    // `begin_mapper_batch` 只处理已加载的表状态，首次写先建空 mapper。
    let source = self.generation_source.clone();
    let alloc_mode = self.alloc_mode;
    let snapshot_source = self.snapshots.generation_source();
    self.table_state(tbl).mapper.get_or_insert_with(|| {
      let mut mapper = PageMapper::new_with_source(source, alloc_mode, 0);
      mapper.set_snapshot_generation_source(snapshot_source);
      mapper
    });
    // Pooled 回收点：写批次之外，仅回收早于所有在途快照的页。
    self.reclaim_retired_now(tbl);
    let batch = self.begin_mapper_batch(tbl);
    let result = self.put_at_inner(tbl, batch_li, now_ms).await;
    self.settle_mapper_batch(tbl, batch, result.is_ok());
    result?;
    // TTL 清理是批次结算后的维护动作；它自带 mapper 批次，不能嵌套在外层 journal 内。
    if inline_ttl {
      self.dispatch_ttl_clean(tbl, now_ms).await;
    }
    // 写入会扩充 data / segment mapping：收尾补记账并按预算驱逐其它表。
    // index 缓存的增长只来自读路径（`&self`），由下一次触摸补记。
    self.refresh_table_bytes(tbl);
    self.evict_over_root_meta(tbl);
    Ok(())
  }

  async fn put_at_inner(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    now_ms: u64,
  ) -> Result<()> {
    let PreparedWriteBatch {
      live_li,
      expired_num: _,
    } = prepare_write_batch(batch_li, now_ms);
    if live_li.is_empty() {
      // 对齐 C++ `WriteTask::FlushManifest` 的空分区标记分支（write_task.cpp:726-733、
      // 769-787）：分区已清空（两棵树均空）且 manifest 已存在时，wal 为空的批也要
      // 整写一份空快照（C++ `SwitchManifest` 等价），把「分区为空」重新持久化 ——
      // C++ 每个收尾必调 `UpdateMeta`，同输入每批重写空快照而 fkv 零写入，同
      // oplog 的 manifest 形态对拍会失败（sem-56）。manifest 未建时 C++
      // `manifest_size_ == 0` 同样不写：首份快照只由真实写批创建。
      if let Some(state) = self.table_map.get(tbl)
        && state.manifest_started
        && state.root == Some(NULL_PAGE)
        && state.ttl_root == NULL_PAGE
      {
        self
          .write_snapshot_manifest(tbl, NULL_PAGE, NULL_PAGE)
          .await?;
      }
      return Ok(());
    }
    // 启用压缩时，首次写为该表建立 DictCompression（无字典，standalone 路径）。
    if let Some(level) = self.compression_level {
      self
        .table_map
        .entry(tbl.clone())
        .or_default()
        .compression
        .get_or_insert_with(|| DictCompression::new(level));
      // 自动字典训练（对齐 C++ `SampleAndBuildDictionaryIfNeeded`）：
      // 样本够了就建字典，同批写入即可走 Dictionary 压缩；字典字节随 manifest 持久化。
      let sample_li: Vec<&[u8]> = live_li
        .iter()
        .filter(|entry| !entry.op.is_delete() && !entry.large_value)
        .map(|entry| entry.value.as_slice())
        .collect();
      if let Some(compression) = self
        .table_map
        .get_mut(tbl)
        .and_then(|state| state.compression.as_mut())
      {
        // 对齐 C++ `BuildDictionary`：首次尝试后即置 `training_attempted`，
        // 失败也不重试、样本随 `take` 丢弃；写路径继续按 standalone/None 压缩。
        let _ = compression.maybe_build_dictionary(&sample_li);
      }
    }
    self.ensure_index_cache(tbl);
    self.ensure_write_cache(tbl);
    let write_cache_bytes = self.write_cache_bytes;
    let compression_enabled = self.compression_level.is_some();
    let state = self.table_map.entry(tbl.clone()).or_default();
    let root_id = state.root.unwrap_or(NULL_PAGE);
    let ttl_root_id = state.ttl_root;
    let first_manifest = !state.manifest_started;
    let source = self.generation_source.clone();
    let alloc_mode = self.alloc_mode;
    let snapshot_source = self.snapshots.generation_source();
    // 受限 branch 的写入上限必须在 `write_batch` **之前**检查（pooled-08）：
    // `write_batch` 内部已把本批新页 flush 到 IO（`fkv_task_write::batch`），
    // `abort_batch` 只回滚 mapping / 分配器，不撤销已写出的字节。高水位已到
    // 块末尾时本批首个分配必然越界；若先写后判（下面的 `mapper_max > limit`），
    // 越界页会落在父表 / 兄弟分支刚用过的 file page id 上（MemStore 的页 key
    // 只有 `(tbl, fp)`），`OutOfSpace` 反而不是安全边界。
    //
    // 高水位 == limit 时保守拒绝：空闲池里可能仍有块内页，但准确判定要看本批
    // 实际分配数；块已满时宁可少写一批，也不能先落盘再回滚。
    if let Some(limit) = self.branch_file_limit
      && self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
        .is_some_and(|mapper| mapper.max_fp_id() >= limit)
    {
      return Err(KvError::OutOfSpace);
    }
    let (new_root, new_ttl_root, ttl_li) = {
      let state = self.table_map.entry(tbl.clone()).or_default();
      let write_cache = state
        .write_cache
        .get_or_insert_with(|| RefCell::new(WritePageCache::new(write_cache_bytes)));
      let mapper = state.mapper.get_or_insert_with(|| {
        let mut mapper = PageMapper::new_with_source(source, alloc_mode, 0);
        mapper.set_snapshot_generation_source(snapshot_source);
        mapper
      });
      let mut segment_mapper = state.segment_mapper.as_mut();
      // 写路径按当前配置门控：关闭压缩时即使 manifest 恢复了历史字典，新写入
      // 也不得使用（落 raw），对齐 C++ `enable_compression` 为 false 时不做
      // `Prepare`；读路径仍用 `state.compression` 解码旧字典压缩页。
      let dict = if compression_enabled {
        state.compression.as_ref()
      } else {
        None
      };
      let segment_size = self
        .large_value_layout
        .map_or(0, |layout| layout.segment_size());
      // 底页归并时收集到的旧 TTL 索引删除项（eng-50）。
      let mut ttl_delete_li = Vec::new();
      // 底页 merge 阶段收集的 TTL upsert（sem-55）：只有 merge 能看到基页的
      // `expire_ts`，才能按 C++ `UpdateTTL` 门控决定是否写索引项。
      let ttl_sink = TtlUpsertSink::new();
      let mut layout = PageLayout::new(
        self.page_size,
        self.data_restart_interval,
        self.index_restart_interval,
        dict,
        self.overflow_pointers,
      )
      .with_now_ms(now_ms);
      layout.data.ttl_upsert_sink = Some(&ttl_sink);
      // 写路径 IO：读先探写缓存（COW 路径上一批刚写出的 root / index / data 页
      // 因此免一次磁盘读），写成功后回填。失效由 `PageMapper::freed_file_page_li`
      // 在批次结算时逐页执行，见 `write_cache` 模块文档。
      let io = CachedWriteIo::new(&self.io, write_cache, self.page_size);
      // `live_li` 在入口已判空，本批必然重写主树。
      let new_root = write_batch(
        &io,
        WriteArg {
          tbl,
          root_id,
          mapper,
          batch_li: &live_li,
          layout,
          segment_mapper: reborrow_segment(&mut segment_mapper),
          segment_size,
          ttl_delete_sink: Some(&mut ttl_delete_li),
        },
      )
      .await?;
      // merge 产出的 TTL upsert（版本号 0，对齐 C++ `UpdateTTL`）与旧索引删除项
      // 合并成一个 TTL 写批；两批 key 因 expire 前缀不同不会互相折叠。
      let mut ttl_li: Vec<BatchEntry> = ttl_sink
        .take()
        .into_iter()
        .map(|(key, expire_ts)| BatchEntry::upsert(ttl_key(expire_ts, &key), Vec::new(), 0))
        .collect();
      ttl_li.extend(ttl_delete_li);
      // `ttl_li` 为空时与 C++ `ApplyTTLBatch` 一致：不碰 TTL 树（sem-55）。
      let new_ttl_root = if ttl_li.is_empty() {
        ttl_root_id
      } else {
        write_batch(
          &io,
          WriteArg {
            tbl,
            root_id: ttl_root_id,
            mapper,
            batch_li: &ttl_li,
            layout: PageLayout::new(
              self.page_size,
              self.data_restart_interval,
              self.index_restart_interval,
              dict,
              self.overflow_pointers,
            )
            .with_now_ms(now_ms),
            segment_mapper: reborrow_segment(&mut segment_mapper),
            segment_size,
            ttl_delete_sink: None,
          },
        )
        .await?
      };
      (new_root, new_ttl_root, ttl_li)
    };
    // 精确检查分支预留区间：本批实际分配的页越界即整体回滚（eng-54）。
    if let Some(limit) = self.branch_file_limit {
      let mapper_max = self
        .table_map
        .get(tbl)
        .and_then(|state| state.mapper.as_ref())
        .map_or(0, PageMapper::max_fp_id);
      if mapper_max > limit {
        return Err(KvError::OutOfSpace);
      }
    }
    if let Some(limit) = self.branch_segment_limit {
      let segment_max = self
        .table_map
        .get(tbl)
        .and_then(|state| state.segment_mapper.as_ref())
        .map_or(0, PageMapper::max_fp_id);
      if segment_max > limit {
        return Err(KvError::OutOfSpace);
      }
    }
    // 先让数据页落盘，再写 manifest（对齐 C++ WaitWrite → SyncData → FlushManifest），
    // 保证 manifest 引用的页在断电后一定可读。
    self.io.sync_data(tbl).await?;

    // 对齐 C++ `WriteTask::FlushManifest` 的 wal 空分支（write_task.cpp:726-733）：
    // 无效果批（空树上的纯 delete / 全过期折叠 delete）不产出任何映射增量，
    // C++ 的 wal 为空时永不 append，也不追加「仅含 branch 元数据」的空 delta
    // record。wal 空且分区非空时不做任何 manifest 写入 —— 当前写路径不可达
    // （非空树的 delete 必重写覆盖页），保留分支只为与 C++ 的门语义逐项对齐。
    let wal_empty = self.table_map.get(tbl).is_some_and(|state| {
      state
        .mapper
        .as_ref()
        .is_none_or(|mapper| mapper.dirty_page_li().is_empty())
        && state
          .segment_mapper
          .as_ref()
          .is_none_or(|mapper| mapper.dirty_page_li().is_empty())
    });
    if wal_empty {
      // 已清空分区（两棵树均空）且 manifest 已存在：整写一份空快照把「分区为空」
      // 重新持久化（C++ `need_empty_snapshot` / SwitchManifest 等价，sem-56）。
      // manifest 未建时 C++ `manifest_size_ == 0` 同样不写（sem-57）：首份快照
      // 只由真实写批创建，无效果批不得凭空建出 manifest 对象；已有 manifest 的
      // 分区在首次访问即自动 reopen（`manifest_started = true`），不会误入本跳过。
      if !first_manifest && new_root == NULL_PAGE && new_ttl_root == NULL_PAGE {
        self
          .write_snapshot_manifest(tbl, new_root, new_ttl_root)
          .await?;
      }
    } else if first_manifest {
      // 追加路径的 branch 元数据刷新由 `append_mapping_log` 自己做；只有首次
      // snapshot 直接取 `branch_metadata`，需要在这里补一次（每批一次 String
      // 克隆 + 区间合并，去掉重复调用）。
      self.refresh_branch_metadata(tbl);
      let record = self.snapshot_record(tbl, new_root, new_ttl_root, None, None, None)?;
      self.write_manifest_record(tbl, &record).await?;
      if let Some(state) = self.table_map.get_mut(tbl) {
        state.manifest_size = record.len();
        state.manifest_started = true;
        if let Some(mapper) = state.mapper.as_mut() {
          mapper.clear_dirty_page_li();
        }
        if let Some(segment_mapper) = state.segment_mapper.as_mut() {
          segment_mapper.clear_dirty_page_li();
        }
      }
      self.clear_dict_dirty(tbl);
    } else {
      self.append_mapping_log(tbl, new_root, new_ttl_root).await?;
    }
    // index page 已被 COW 重写，缓存作废。必须等 manifest 落盘成功再失效 / 重标：
    // 失败路径会把本批 mapping 回滚到批次前（`abort_batch`），此时失效既白丢
    // 仍然有效的旧条目，也把缓存 baseline 提前抬到了回滚后并不会被用到的代际。
    // 失败时 `settle_mapper_batch` 已清空缓存并把 baseline 抬到新的 live 代际，
    // 在途旧快照的回填被拒，无需这里兜底。
    self.invalidate_index_cache(tbl);
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.root = Some(new_root);
      state.ttl_root = new_ttl_root;
    }
    self.note_ttl_batch(tbl, &ttl_li);
    Ok(())
  }

  pub async fn delete(&mut self, tbl: &TableIdent, key: &[u8], timestamp: u64) -> Result<()> {
    self
      .put(tbl, &[BatchEntry::delete(key.to_vec(), timestamp)])
      .await
  }
}
/// 大 value 写入目标：plain 直接按 segment 写；fixed 先落调用方注册 buffer 再写。
///
/// 两条路径只在“value 从哪取”上不同，提交 / 回滚骨架由 `put_large_impl` 统一。
trait LargeSink<I: SegmentIo> {
  /// 写入前的长度校验与搬运；plain 路径为 no-op。
  fn prepare(&mut self, value: &[u8]) -> Result<()>;

  async fn write(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &mut PageMapper,
    layout: SegmentLayout,
    value: &[u8],
    metadata: &[u8],
  ) -> Result<WrittenLargeValue>;
}

struct PlainSink;

impl<I: SegmentIo> LargeSink<I> for PlainSink {
  fn prepare(&mut self, _value: &[u8]) -> Result<()> {
    Ok(())
  }

  async fn write(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &mut PageMapper,
    layout: SegmentLayout,
    value: &[u8],
    metadata: &[u8],
  ) -> Result<WrittenLargeValue> {
    write_large_value(io, tbl, mapper, layout, value, metadata).await
  }
}

/// fixed buffer 写入目标：value 先拷入 buffer，尾部保留区不写出。
struct FixedSink<'a> {
  buf: &'a mut FixedBuffer,
  value_len: usize,
}

impl<I: FixedSegmentIo> LargeSink<I> for FixedSink<'_> {
  fn prepare(&mut self, value: &[u8]) -> Result<()> {
    let dst = self.buf.as_mut_slice().ok_or(KvError::Busy)?;
    if value.len() > dst.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..value.len()].copy_from_slice(value);
    self.value_len = value.len();
    Ok(())
  }

  async fn write(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &mut PageMapper,
    layout: SegmentLayout,
    _value: &[u8],
    metadata: &[u8],
  ) -> Result<WrittenLargeValue> {
    write_value_fixed_with_len(io, tbl, mapper, layout, self.buf, self.value_len, metadata).await
  }
}

/// 命中 large path 的 entry：只替换 value / metadata，避免再复制整份原始大 value。
fn large_entry(entry: &BatchEntry, content: Vec<u8>) -> BatchEntry {
  BatchEntry {
    key: entry.key.clone(),
    value: content,
    timestamp: entry.timestamp,
    expire_ts: entry.expire_ts,
    op: entry.op,
    large_value: true,
    overflow: entry.overflow,
    metadata: Vec::new(),
  }
}

impl<I: PageIo + ManifestIo + SegmentIo> Store<I> {
  /// 按 key 折叠本批写入，保留 timestamp 最大的版本（相等时后出现者胜）。
  ///
  /// 与 `prepare_write_batch` 的主树折叠语义一致；`put_large` 必须在落 segment
  /// 之前先折叠，否则被折叠条目的段链既无树引用也不会 `free_value`，映射与空间
  /// 要泄漏到下一次段压缩（R4-05）。
  fn fold_latest(batch_li: &[BatchEntry]) -> Vec<&BatchEntry> {
    let mut winner_map: BTreeMap<&[u8], &BatchEntry> = BTreeMap::new();
    for entry in batch_li {
      match winner_map.entry(entry.key.as_slice()) {
        Entry::Occupied(mut slot) => {
          if entry.timestamp >= slot.get().timestamp {
            slot.insert(entry);
          }
        }
        Entry::Vacant(slot) => {
          slot.insert(entry);
        }
      }
    }
    winner_map.into_values().collect()
  }

  /// Store 级大 value 写入：超过阈值的 upsert 先写 segment 文件，再写 data page。
  ///
  /// 有意差异（sem-46）：覆盖同一 key 的大 value 时，本实现先为新 value 分配
  /// segment 逻辑页，旧 value 的段在 data page merge 的 `free_replaced_values`
  /// 里才释放；C++ `BatchWriteTask::ApplyBatch` 先在 COW meta 上
  /// `DelLargeValue`，随后 `WriteLargeValue` 从空闲链取回刚释放的逻辑页。因此
  /// 同一负载下 fkv 的 segment 映射表稳态长度约为 live 段数的 2 倍，manifest
  /// segment 段的逻辑 page id 集合 / 长度与 C++ 不同。
  ///
  /// 不影响可见数据与互读：逻辑页 id 只是本 manifest 内部的索引，两侧都按
  /// 「已提交 manifest 的 segment mapping → 物理段页」读取。崩溃安全也不依赖
  /// 释放 / 分配的先后：段 mapper 恒 `Append`（旧物理段页永不被覆盖），data
  /// 页落盘并 sync 后才追加 manifest（本模块 `write_batch`），失败批由 mapper
  /// journal 整体回滚，未提交的段由 GC 回收（test-06）。对齐 C++ 需要把「旧段
  /// 是否真被本批替换」的判定前移到 merge 之外（delete / 小 value / 过期覆盖
  /// 也会释放旧段），并抑制 `free_replaced_values` 对已复用槽的二次释放，
  /// 属布局级收益、风险更大，故登记为有意差异。
  pub async fn put_large(&mut self, tbl: &TableIdent, batch_li: &[BatchEntry]) -> Result<()> {
    self.put_large_impl(tbl, batch_li, &mut PlainSink).await
  }

  /// 两条大 value 写路径共享的骨架：开批、逐条落 segment、统一提交 / 回滚。
  async fn put_large_impl<S: LargeSink<I>>(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    sink: &mut S,
  ) -> Result<()> {
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    // 与 `put_at` 同一校验点前移：空 / 超长 key 必须在落 segment 之前拒绝，
    // 否则会在写完整批段页后才 abort，白写且可能留下孤儿段。
    self.check_batch_key_li(batch_li)?;
    // 与 `put_at` 用同一时间基准判定过期，避免折叠后仍为过期 upsert 落段。
    let now_ms = now_ms();
    let folded_li = Self::fold_latest(batch_li);
    let mut write_li = Vec::with_capacity(folded_li.len());
    // segment 分配与 data page 写入作为一个批次回滚：`self.put` 失败时
    // 释放本次分配的 segment 映射。
    let source = self.generation_source.clone();
    let snapshot_source = self.snapshots.generation_source();
    let batch = {
      let segment_mapper = self
        .table_map
        .entry(tbl.clone())
        .or_default()
        .segment_mapper
        .get_or_insert_with(|| {
          // 段 mapper 恒用 `Append`（对齐 C++ `BatchWriteTask::EnsureSegmentMapper`
          // 无条件安装 `AppendAllocator`）：段页释放后直接作废、永不复用，
          // 因此不存在 retired 队列，也不受 `data_append_mode` 影响（pooled-02）。
          let mut mapper = PageMapper::new_with_source(source, AllocMode::Append, 0);
          mapper.set_snapshot_generation_source(snapshot_source);
          mapper
        });
      MapperBatch {
        data: false,
        segment: segment_mapper.begin_batch(),
      }
    };
    let mut result = Ok(());
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for (seq, entry) in folded_li.into_iter().enumerate() {
      // 大 value 批次可能很多条，每条都可能写整条 segment 链；按步长让出。
      yield_budget.maybe_yield_every(seq).await;
      // 过期 upsert 由 `put_at` 折叠成 delete（并清 TTL 索引），不落段。
      if entry.op == WriteOp::Upsert
        && !entry.large_value
        && entry.value.len() >= self.large_value_threshold
        && !is_expired(entry.expire_ts, now_ms)
      {
        if let Err(err) = sink.prepare(&entry.value) {
          result = Err(err);
          break;
        }
        // 比树中版本更旧的 upsert 会被 merge 丢弃；提前跳过，避免为落败版本
        // 分配 segment（映射 / 磁盘 / branch file id 预留区间永久泄漏）。
        let base_ts = match self.base_timestamp(tbl, &entry.key).await {
          Ok(ts) => ts,
          Err(err) => {
            // 不能 `?` 直接返回：segment MapperBatch 已开场，必须走到统一结算点。
            result = Err(err);
            break;
          }
        };
        if let Some(base_ts) = base_ts
          && entry.timestamp < base_ts
        {
          continue;
        }
        let Some(segment_mapper) = self
          .table_map
          .get_mut(tbl)
          .and_then(|state| state.segment_mapper.as_mut())
        else {
          result = Err(KvError::ResourceMissing);
          break;
        };
        // 与 data 侧同口径：段页在 `sink.write` 内落盘，越界检查必须在分配
        // 之前（pooled-08）。段 mapper 恒 Append，块满后继续写只会把页写到
        // 兄弟分支的 segment 区间上。
        if self
          .branch_segment_limit
          .is_some_and(|limit| segment_mapper.max_fp_id() >= limit)
        {
          result = Err(KvError::OutOfSpace);
          break;
        }
        match sink
          .write(
            &self.io,
            tbl,
            segment_mapper,
            layout,
            &entry.value,
            &entry.metadata,
          )
          .await
        {
          Ok(written) => write_li.push(large_entry(entry, written.content)),
          Err(err) => {
            result = Err(err);
            break;
          }
        }
      } else {
        // 未命中 large path 的 value 必小于阈值（或已是编码后的 content），
        // 复制成本有界；命中的大 value 走 `large_entry` 不复制。
        write_li.push(entry.clone());
      }
    }
    if result.is_ok()
      && let Some(limit) = self.branch_segment_limit
    {
      let segment_max = self
        .table_map
        .get(tbl)
        .and_then(|state| state.segment_mapper.as_ref())
        .map_or(0, PageMapper::max_fp_id);
      if segment_max > limit {
        result = Err(KvError::OutOfSpace);
      }
    }
    if result.is_ok() {
      // 段 journal 尚未结算：TTL 清理交回本函数在结算后执行。
      result = self.put_at_with_ttl(tbl, &write_li, now_ms, false).await;
    }
    self.settle_mapper_batch(tbl, batch, result.is_ok());
    // 段批次已结算，此时清理自带 mapper journal 不会与外层交织回滚。
    if result.is_ok() {
      self.dispatch_ttl_clean(tbl, now_ms).await;
    }
    result
  }
}

impl<I: PageIo + ManifestIo + FixedSegmentIo> Store<I> {
  /// 与 `put_large` 相同，但大 value 通过调用方注册的 fixed buffer 写出。
  pub async fn put_large_fixed(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    buf: &mut FixedBuffer,
  ) -> Result<()> {
    let mut sink = FixedSink { buf, value_len: 0 };
    self.put_large_impl(tbl, batch_li, &mut sink).await
  }
}
