//! 点读、floor、范围 / 分页扫描与大 value 读取。

use fkv_error::{KvError, Result};
use fkv_io::{FixedBuffer, FixedSegmentIo, PageIo, SegmentIo};
use fkv_large::{
  ManagedReadResult, SegmentLayout, decode_metadata_only, read_value as read_large_value,
  read_value_fixed,
};
use fkv_mapping::{MappingSnapshot, PageMapper};
use fkv_task_read::{ReadEntry, ReadFloorEntry, read_entry, read_floor_entry};
use fkv_task_scan::{
  PageOption, ScanArg, ScanOption, ScanTree, scan_count_with_budget, scan_entries_with_budget,
  scan_page_with_budget,
};
use fkv_types::{NULL_PAGE, TableIdent, now_ms};

use crate::{
  FloorValue, GetEntry, KvEntry, KvEntryPage, LargeValue, LargeValueInto, ScanPageResult, Store,
  page_cache::CachedReadIo,
};

impl<I: PageIo> Store<I> {
  /// 读路径 IO：在真实 `PageIo` 外面包一层读写共用页缓存；index 页恒走缓存，
  /// data 页按 [`Store::enable_data_page_cache`] 分流。
  ///
  /// 缓存实例与写路径共用（`TableState::page_cache`）：写批刚写出的页，紧接着的
  /// 读请求直接命中。表没有缓存实例（预算 `0` / 表未打开）时完全直通。
  /// 只覆盖 `PageIo` 的页读；大 value 的 segment 读走 `SegmentIo`，不在此列。
  fn read_io(&self, tbl: &TableIdent) -> CachedReadIo<'_, I> {
    CachedReadIo::new(
      &self.io,
      self.page_cache(tbl),
      self.page_size,
      self.enable_data_page_cache,
    )
  }

  /// 读上下文：快照 + 字典。
  ///
  /// 快照由请求入口取一次并在整个点读 / floor 内复用；上下文不携带当前
  /// `PageMapper`，避免「快照 + 当前映射」混用（27b）。
  pub(crate) fn read_ctx<'a>(
    &'a self,
    tbl: &TableIdent,
    snapshot: &'a MappingSnapshot,
  ) -> fkv_task_read::ReadCtx<'a> {
    let state = self.table_map.get(tbl);
    fkv_task_read::ReadCtx {
      snapshot,
      dict: state.and_then(|state| state.compression.as_ref()),
      ttl_filter_now_ms: None,
      yield_budget_us: self.yield_budget_us,
    }
  }

  /// 点读上下文：在 `read_ctx` 基础上带上 TTL 下推阈值。
  ///
  /// 过期项在 value 物化前即返回 `None`，省掉 overflow 链 IO 与解压；
  /// `ttl_read_filter == false` 时阈值仍为 `None`，由调用方事后判定。
  fn read_ctx_at<'a>(
    &'a self,
    tbl: &TableIdent,
    snapshot: &'a MappingSnapshot,
    now_ms: u64,
  ) -> fkv_task_read::ReadCtx<'a> {
    fkv_task_read::ReadCtx {
      ttl_filter_now_ms: self.ttl_read_filter.then_some(now_ms),
      ..self.read_ctx(tbl, snapshot)
    }
  }

  /// data 树扫描参数：分页 / 物化 / 计数三个扫描入口共用，仅 `max_entries`
  /// 不同（收敛方式同 [`Self::read_ctx`]）。
  fn scan_arg<'a>(
    &'a self,
    tbl: &'a TableIdent,
    snapshot: &'a MappingSnapshot,
    begin: &'a [u8],
    end: &'a [u8],
    max_entries: usize,
    now_ms: u64,
  ) -> ScanArg<'a> {
    let state = self.table_map.get(tbl);
    ScanArg {
      tbl,
      snapshot,
      tree: ScanTree::Data,
      begin,
      end,
      page_size: self.page_size,
      max_entries,
      dict: state.and_then(|state| state.compression.as_ref()),
      ttl_filter_now_ms: self.ttl_read_filter.then_some(now_ms),
    }
  }

  pub async fn get(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    self.get_at(tbl, key, now_ms()).await
  }

  pub async fn get_at(&self, tbl: &TableIdent, key: &[u8], now_ms: u64) -> Result<Option<Vec<u8>>> {
    (async move {
      // 一次请求取一次快照：点读全程（含 overflow 链）只用它，不受并发写影响。
      let Some(snapshot) = self.snapshot(tbl) else {
        return Ok(None);
      };
      if snapshot.root_id() == NULL_PAGE {
        return Ok(None);
      }
      let io = self.read_io(tbl);
      let entry = read_entry(
        &io,
        tbl,
        key,
        self.page_size,
        self.read_ctx_at(tbl, &snapshot, now_ms),
      )
      .await;
      let Some(entry) = entry? else {
        return Ok(None);
      };
      // large value 的 `entry.value` 是 segment 指针 + metadata 编码，不是用户数据；
      // 绝不能透传给 `get` / `read`（issue 15）。用 `get_large*` 读取。
      if entry.is_large_value {
        return Err(KvError::LargeValueUnsupported);
      }
      Ok(Some(entry.value))
    })
    .await
  }

  pub async fn read(&self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.read_at(tbl, key, now_ms()).await
  }

  pub async fn read_at(&self, tbl: &TableIdent, key: &[u8], now_ms: u64) -> Result<Vec<u8>> {
    self
      .get_at(tbl, key, now_ms)
      .await?
      .ok_or(KvError::NotFound)
  }

  pub async fn scan(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self.scan_at(tbl, begin, end, max_entries, now_ms()).await
  }

  pub async fn scan_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self
      .scan_opt(tbl, begin, end, max_entries, now_ms, ScanOption::default())
      .await
  }

  pub async fn scan_with_option(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    option: ScanOption,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    self
      .scan_opt(tbl, begin, end, max_entries, now_ms(), option)
      .await
  }

  pub async fn scan_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanOption,
    page: PageOption,
  ) -> Result<ScanPageResult> {
    self
      .scan_page_at(tbl, begin, end, now_ms(), option, page)
      .await
  }

  pub async fn scan_page_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    now_ms: u64,
    option: ScanOption,
    page: PageOption,
  ) -> Result<ScanPageResult> {
    let page = self
      .scan_entry_page_at(tbl, begin, end, now_ms, option, page)
      .await?;
    Ok(ScanPageResult {
      entry_li: page
        .entry_li
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect(),
      has_remaining: page.has_remaining,
    })
  }

  /// 分页扫描，保留每条 entry 的 `expire_ts`。
  pub async fn scan_entry_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanOption,
    page: PageOption,
  ) -> Result<KvEntryPage> {
    self
      .scan_entry_page_at(tbl, begin, end, now_ms(), option, page)
      .await
  }

  pub async fn scan_entry_page_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    now_ms: u64,
    option: ScanOption,
    page: PageOption,
  ) -> Result<KvEntryPage> {
    // 分页扫描同样是读请求：一次请求取一次快照，页链全程复用它。
    (async move {
      let Some(snapshot) = self.snapshot(tbl) else {
        return Ok(KvEntryPage::default());
      };
      if snapshot.root_id() == NULL_PAGE {
        return Ok(KvEntryPage::default());
      }
      // TTL 过滤在叶子迭代器层完成：过期条目不读 overflow、不占分页配额。
      // 单次扫描从 root 下降到叶子后顺序推进，不再因过滤/字节超限重查整棵树。
      let io = self.read_io(tbl);
      let task_page = scan_page_with_budget(
        &io,
        self.scan_arg(tbl, &snapshot, begin, end, 0, now_ms),
        option,
        page,
        self.yield_budget_us,
      )
      .await?;
      Ok(KvEntryPage {
        entry_li: task_page.entry_li.into_iter().map(KvEntry::from).collect(),
        has_remaining: task_page.has_remaining,
      })
    })
    .await
  }

  pub async fn scan_opt(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
    option: ScanOption,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    Ok(
      self
        .scan_entry_opt(tbl, begin, end, max_entries, now_ms, option)
        .await?
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect(),
    )
  }

  /// 范围扫描，保留每条 entry 的 `expire_ts`；TTL 过滤口径与 `scan` 一致。
  pub async fn scan_entry(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<KvEntry>> {
    self
      .scan_entry_at(tbl, begin, end, max_entries, now_ms())
      .await
  }

  pub async fn scan_entry_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
  ) -> Result<Vec<KvEntry>> {
    self
      .scan_entry_opt(tbl, begin, end, max_entries, now_ms, ScanOption::default())
      .await
  }

  pub async fn scan_entry_opt(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    now_ms: u64,
    option: ScanOption,
  ) -> Result<Vec<KvEntry>> {
    // 范围扫描也是读请求：一次请求取一次快照，与点读共用 descend_step 的写法。
    (async move {
      let Some(snapshot) = self.snapshot(tbl) else {
        return Ok(Vec::new());
      };
      if snapshot.root_id() == NULL_PAGE {
        return Ok(Vec::new());
      }
      // 两个上限都下推到叶子迭代器：`max_entries` 逐条限量（不整区间物化），
      // TTL 过滤在解 overflow / 解压之前完成，过期条目不读 value、不占配额。
      // 与 `scan_entry_page_at` 同一口径，见 eng-52 / eng-64 / perf-84。
      let io = self.read_io(tbl);
      let entry_li = scan_entries_with_budget(
        &io,
        self.scan_arg(tbl, &snapshot, begin, end, max_entries, now_ms),
        option,
        self.yield_budget_us,
      )
      .await?;
      Ok(entry_li.into_iter().map(KvEntry::from).collect())
    })
    .await
  }

  /// 全区间扫描计数：与 `scan` / `scan_entry` 物化后取 `len()` 完全等价
  /// （同一 begin/end 边界、同一 TTL 过期过滤与 large value 拒绝口径），
  /// 但不物化 key/value、不读 overflow 链、不解压。
  ///
  /// 全表 `len()` 只需沿叶子页链推进并累加 entry 数，省掉整表 key/value
  /// 拷贝与 overflow / 解压 IO；计数结果与 `scan(tbl, begin, end, 0).len()`
  /// 必须一致（见回归测试 `scan_count_matches_materialized_scan_with_deletes`）。
  /// 含 large value 的表会与 `scan` 一样返回 `LargeValueUnsupported`。
  pub async fn scan_count(&self, tbl: &TableIdent, begin: &[u8], end: &[u8]) -> Result<usize> {
    self.scan_count_at(tbl, begin, end, now_ms()).await
  }

  /// 同 [`Self::scan_count`]，`now_ms` 显式传入（TTL 阈值可控）。
  pub async fn scan_count_at(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    now_ms: u64,
  ) -> Result<usize> {
    // 计数也是读请求：一次请求取一次快照，与 scan* 共用 descend_step 的写法。
    (async move {
      let Some(snapshot) = self.snapshot(tbl) else {
        return Ok(0);
      };
      if snapshot.root_id() == NULL_PAGE {
        return Ok(0);
      }
      // 计数与物化扫描共用同一遍历：max_entries=0 不限量，TTL 过滤在叶子
      // 迭代器层完成，过期条目不计数也不占配额。
      let io = self.read_io(tbl);
      scan_count_with_budget(
        &io,
        self.scan_arg(tbl, &snapshot, begin, end, 0, now_ms),
        ScanOption::default(),
        self.yield_budget_us,
      )
      .await
    })
    .await
  }
}
/// 点读 entry 的版本元数据，与解析出的 value 一起交付给目标。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EntryMeta {
  timestamp: u64,
  expire_ts: u64,
}

/// 大 value 读取目标：owned 分配 Vec 返回，buffer 直接读进调用方 FixedBuffer。
trait ValueDst<I: SegmentIo> {
  type Out;

  /// 非 large value：直接交付 data page 里的原始 value（owned 路径零拷贝搬运）。
  fn inline(&mut self, value: Vec<u8>, meta: EntryMeta) -> Result<Self::Out>;

  /// large value：解析编码并按 segment 映射还原。
  async fn large(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    layout: SegmentLayout,
    content: &[u8],
    meta: EntryMeta,
  ) -> Result<Self::Out>;
}

struct OwnedDst;

impl<I: SegmentIo> ValueDst<I> for OwnedDst {
  type Out = LargeValue;

  fn inline(&mut self, value: Vec<u8>, meta: EntryMeta) -> Result<LargeValue> {
    Ok(LargeValue {
      value,
      metadata: Vec::new(),
      timestamp: meta.timestamp,
      expire_ts: meta.expire_ts,
    })
  }

  async fn large(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    layout: SegmentLayout,
    content: &[u8],
    meta: EntryMeta,
  ) -> Result<LargeValue> {
    let read = read_large_value(io, tbl, mapper, layout, content).await?;
    Ok(LargeValue {
      value: read.value,
      metadata: read.metadata,
      timestamp: meta.timestamp,
      expire_ts: meta.expire_ts,
    })
  }
}

struct BufferDst<'a>(&'a mut FixedBuffer);

impl<I: FixedSegmentIo> ValueDst<I> for BufferDst<'_> {
  type Out = LargeValueInto;

  fn inline(&mut self, value: Vec<u8>, meta: EntryMeta) -> Result<LargeValueInto> {
    let dst = self.0.as_mut_slice().ok_or(KvError::Busy)?;
    if value.len() > dst.len() {
      return Err(KvError::InvalidArgs);
    }
    dst[..value.len()].copy_from_slice(&value);
    Ok(LargeValueInto {
      value_len: value.len(),
      metadata: Vec::new(),
      timestamp: meta.timestamp,
      expire_ts: meta.expire_ts,
    })
  }

  async fn large(
    &mut self,
    io: &I,
    tbl: &TableIdent,
    mapper: &PageMapper,
    layout: SegmentLayout,
    content: &[u8],
    meta: EntryMeta,
  ) -> Result<LargeValueInto> {
    let read: ManagedReadResult =
      read_value_fixed(io, tbl, mapper, layout, content, self.0).await?;
    Ok(LargeValueInto {
      value_len: read.header.actual_len as usize,
      metadata: read.metadata,
      timestamp: meta.timestamp,
      expire_ts: meta.expire_ts,
    })
  }
}

impl<I: PageIo + SegmentIo> Store<I> {
  /// 点读 entry（TTL 下推）；表未初始化返回 `None`。
  ///
  /// 快照由调用方取一次并在 entry / large value 解析期间复用。
  async fn read_entry_at(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    key: &[u8],
    now_ms: u64,
  ) -> Result<Option<ReadEntry>> {
    let io = self.read_io(tbl);
    read_entry(
      &io,
      tbl,
      key,
      self.page_size,
      self.read_ctx_at(tbl, snapshot, now_ms),
    )
    .await
  }

  /// 当前树中该 key 的版本时间戳（忽略 TTL 过滤，`now=0`）：大 value 写入前
  /// 用它判断本批是否会被 merge 丢弃，避免为落败版本分配 segment 造成泄漏。
  pub(crate) async fn base_timestamp(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<u64>> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(None);
    };
    Ok(
      self
        .read_entry_at(tbl, &snapshot, key, 0)
        .await?
        .map(|entry| entry.timestamp),
    )
  }

  /// 统一 entry → value 还原：非 large 直接交付，large 按快照的 segment 映射读回。
  async fn resolve_value<D: ValueDst<I>>(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    value: Vec<u8>,
    is_large_value: bool,
    meta: EntryMeta,
    dst: &mut D,
  ) -> Result<D::Out> {
    if !is_large_value {
      return dst.inline(value, meta);
    }
    let Some(layout) = self.large_value_layout else {
      return Err(KvError::InvalidArgs);
    };
    let Some(segment_mapper) = snapshot.segment_mapper() else {
      return Err(KvError::ResourceMissing);
    };
    dst
      .large(&self.io, tbl, segment_mapper, layout, &value, meta)
      .await
  }

  /// 最大且 <= key 的 entry；不存在返回 `NotFound`。
  pub async fn floor(&self, tbl: &TableIdent, key: &[u8]) -> Result<FloorValue> {
    // floor 会沿 prev 页链回退，同样是读请求：一次请求取一次快照。
    (async move {
      let Some(snapshot) = self.snapshot(tbl) else {
        return Err(KvError::NotFound);
      };
      let io = self.read_io(tbl);
      let Some(entry) = read_floor_entry(
        &io,
        tbl,
        key,
        self.page_size,
        self.read_ctx_at(tbl, &snapshot, now_ms()),
      )
      .await?
      else {
        return Err(KvError::NotFound);
      };
      let ReadFloorEntry {
        key,
        value,
        timestamp,
        expire_ts,
        is_large_value,
      } = entry;
      let read = self
        .resolve_value(
          tbl,
          &snapshot,
          value,
          is_large_value,
          EntryMeta {
            timestamp,
            expire_ts,
          },
          &mut OwnedDst,
        )
        .await?;
      Ok(FloorValue {
        key,
        value: read.value,
        metadata: read.metadata,
        timestamp,
        expire_ts,
      })
    })
    .await
  }

  /// 点读 entry：对齐 C++ `ReadRequest` 的默认（`std::monostate`）形态。
  ///
  /// 非 large value 返回完整 value；large value **不读取 segment**，只解析
  /// data page 里的编码头与 metadata trailer。`get` / `read` 对 large value
  /// 仍返回 `LargeValueUnsupported`（issue 15），需要完整 value 的调用方用
  /// `get_large` / `get_large_into`。
  pub async fn get_entry(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<GetEntry>> {
    self.get_entry_at(tbl, key, now_ms()).await
  }

  pub async fn get_entry_at(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    now_ms: u64,
  ) -> Result<Option<GetEntry>> {
    // 与 `get` 同一条点读路径：快照取一次，TTL 过滤阈值与点读一致。
    (async move {
      let Some(snapshot) = self.snapshot(tbl) else {
        return Ok(None);
      };
      let Some(ReadEntry {
        value,
        timestamp,
        expire_ts,
        is_large_value,
      }) = self.read_entry_at(tbl, &snapshot, key, now_ms).await?
      else {
        return Ok(None);
      };
      if !is_large_value {
        return Ok(Some(GetEntry {
          value,
          metadata: Vec::new(),
          timestamp,
          expire_ts,
          is_large_value: false,
        }));
      }
      // metadata-only：只解析头与 trailer，segment 指针不进入输出。
      let Some(layout) = self.large_value_layout else {
        return Err(KvError::InvalidArgs);
      };
      let (_, metadata) = decode_metadata_only(&value, layout.segment_size())?;
      Ok(Some(GetEntry {
        value: Vec::new(),
        metadata,
        timestamp,
        expire_ts,
        is_large_value: true,
      }))
    })
    .await
  }

  pub async fn get_large(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValue>> {
    self.get_large_at(tbl, key, now_ms()).await
  }

  pub async fn get_large_at(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    now_ms: u64,
  ) -> Result<Option<LargeValue>> {
    // 大 value 读含 segment 映射与溢出链 IO：一次请求取一次快照，段映射全程一致。
    (async move {
      let Some(snapshot) = self.snapshot(tbl) else {
        return Ok(None);
      };
      let Some(ReadEntry {
        value,
        timestamp,
        expire_ts,
        is_large_value,
      }) = self.read_entry_at(tbl, &snapshot, key, now_ms).await?
      else {
        return Ok(None);
      };
      Ok(Some(
        self
          .resolve_value(
            tbl,
            &snapshot,
            value,
            is_large_value,
            EntryMeta {
              timestamp,
              expire_ts,
            },
            &mut OwnedDst,
          )
          .await?,
      ))
    })
    .await
  }
}

impl<I: PageIo + FixedSegmentIo> Store<I> {
  pub async fn get_large_into(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    buf: &mut FixedBuffer,
  ) -> Result<Option<LargeValueInto>> {
    self.get_large_into_at(tbl, key, now_ms(), buf).await
  }

  /// 大 value 直接读入 fixed buffer；返回有效长度、metadata 与 entry 元数据。
  pub async fn get_large_into_at(
    &self,
    tbl: &TableIdent,
    key: &[u8],
    now_ms: u64,
    buf: &mut FixedBuffer,
  ) -> Result<Option<LargeValueInto>> {
    // 与 `get_large_at` 同一条读路径：一次请求取一次快照。
    (async move {
      let Some(snapshot) = self.snapshot(tbl) else {
        return Ok(None);
      };
      let Some(ReadEntry {
        value,
        timestamp,
        expire_ts,
        is_large_value,
      }) = self.read_entry_at(tbl, &snapshot, key, now_ms).await?
      else {
        return Ok(None);
      };
      Ok(Some(
        self
          .resolve_value(
            tbl,
            &snapshot,
            value,
            is_large_value,
            EntryMeta {
              timestamp,
              expire_ts,
            },
            &mut BufferDst(buf),
          )
          .await?,
      ))
    })
    .await
  }
}
