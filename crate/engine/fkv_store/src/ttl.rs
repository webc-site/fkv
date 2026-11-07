//! TTL 索引维护、过期清理与延迟清理。

use std::cell::RefCell;

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo};
use fkv_mapping::MappingSnapshot;
use fkv_task_read::{YieldBudget, read_entry};
use fkv_task_scan::{
  PageOption, ScanArg, ScanOption, ScanTree, ScannedEntry, scan_entries, scan_page_with_budget,
};
use fkv_task_write::{PageLayout, WriteArg, parse_ttl_key, ttl_key, write_batch};
use fkv_types::{NULL_PAGE, PageId, TableIdent, WriteOp, now_ms};

use crate::{
  CLEAN_EXPIRED_MAX_ENTRIES, CleanExpiredStat, Store, reborrow_segment,
  write_cache::{CachedWriteIo, WritePageCache},
};

/// 单次 TTL 探测最多跳过的坏 key 数。
///
/// 坏 key 会被 `clean_expired` 一并删除，因此每轮清理都会向前推进；上界只
/// 用来封顶单轮的长尾读取，命中后把 `next_expire_ts` 压回当前时刻驱动下一轮
/// （r7a-core-05）。
const TTL_BAD_KEY_SKIP_MAX: usize = 16;

/// TTL 探测的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtlPeek {
  /// 首个可解析 TTL 条目的到期时间。
  Expire(u64),
  /// 探测区间内已无条目。
  Empty,
  /// 连续坏 key 达到 [`TTL_BAD_KEY_SKIP_MAX`]，本轮放弃探测。
  Truncated,
}

/// 过期清理的删除版本号：至少要压过被删条目自身的 ts。
///
/// 用户 ts 由客户端提供，可能是微秒 / 纳秒量级（远大于毫秒墙钟）；若直接用
/// 调用方传入的毫秒 `now_ms` 作删除 ts，折叠时 delete 因 ts 更小而落败，
/// 过期条目永不物理删除、空间无界增长。取 `max(now_ms, 条目 ts)`：同 ts 时
/// 后项（delete）胜，因此纳秒量级的条目也能删掉；而毫秒量级客户端仍沿用
/// 墙钟作 tombstone，不会被未来写入的毫秒 ts 反压。
fn expired_delete_ts(entry_ts: u64) -> u64 {
  now_ms().max(entry_ts)
}

impl<I: PageIo> Store<I> {
  pub fn pending_ttl_len(&self) -> usize {
    self
      .table_map
      .values()
      .filter(|state| state.ttl_pending)
      .count()
  }

  fn mark_ttl_pending(&mut self, tbl: &TableIdent, now_ms: u64) {
    if let Some(state) = self.table_map.get_mut(tbl)
      && state.next_expire != 0
      && state.next_expire <= now_ms
    {
      state.ttl_pending = true;
    }
  }
}

impl<I: PageIo + ManifestIo> Store<I> {
  fn note_next_expire(&mut self, tbl: &TableIdent, expire_ts: u64) {
    if expire_ts == 0 {
      return;
    }
    let state = self.table_state(tbl);
    if state.next_expire == 0 || expire_ts < state.next_expire {
      state.next_expire = expire_ts;
    }
  }

  fn record_next_expire(&mut self, tbl: &TableIdent, next_expire_ts: u64) {
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.next_expire = next_expire_ts;
    }
  }

  pub(crate) fn note_ttl_batch(&mut self, tbl: &TableIdent, ttl_li: &[BatchEntry]) {
    for entry in ttl_li {
      if entry.op == WriteOp::Upsert
        && let Ok((expire_ts, _)) = parse_ttl_key(&entry.key)
      {
        self.note_next_expire(tbl, expire_ts);
      }
    }
  }

  /// 写后触发：只在已知最早过期时间已到时扫描 TTL 树，对齐 C++ TriggerTTL。
  async fn maybe_clean_expired(&mut self, tbl: &TableIdent, now_ms: u64) -> Result<()> {
    let next = self.table_map.get(tbl).map_or(0, |state| state.next_expire);
    if next == 0 || next > now_ms {
      return Ok(());
    }
    self.clean_expired(tbl, now_ms).await?;
    Ok(())
  }

  /// 执行所有已标记 table 的 TTL 清理（对齐 C++ 独立 CleanExpired 任务）。
  ///
  /// 逐表清理成功后才清该表的 pending 标记；中途失败时失败表及其后的表
  /// 保留 pending，下一轮 drain 继续，不会因一次错误丢失待清理状态。
  ///
  /// 单次 `clean_expired` 有 [`CLEAN_EXPIRED_MAX_ENTRIES`] 上界，命中上界时它
  /// 返回 `next_expire_ts = now`：只有 `next_expire_ts` 仍未到期（或清理无进展）
  /// 才清 pending，否则继续下一轮，避免写停止后过期项滞留在 TTL 树里。
  pub async fn drain_pending_ttl_at(&mut self, now_ms: u64) -> Result<usize> {
    let table_li: Vec<TableIdent> = self
      .table_map
      .iter()
      .filter(|(_, state)| state.ttl_pending)
      .map(|(tbl, _)| tbl.clone())
      .collect();
    let mut cleaned = 0;
    for tbl in table_li {
      loop {
        let stat = self.clean_expired(&tbl, now_ms).await?;
        let more = stat.next_expire_ts != 0 && stat.next_expire_ts <= now_ms;
        // 无进展保护：理论上命中上界必删条目，异常实现下也绝不空转。
        let progressed = stat.ttl_deleted != 0 || stat.deleted != 0;
        if let Some(state) = self.table_map.get_mut(&tbl) {
          state.ttl_pending = more;
        }
        if !more || !progressed {
          break;
        }
      }
      cleaned += 1;
    }
    Ok(cleaned)
  }

  pub async fn drain_pending_ttl(&mut self) -> Result<usize> {
    self.drain_pending_ttl_at(now_ms()).await
  }

  /// 写后 TTL 维护：延迟模式只标记 pending，内联模式立即清理且失败仅告警。
  pub(crate) async fn dispatch_ttl_clean(&mut self, tbl: &TableIdent, now_ms: u64) {
    if self.defer_ttl {
      self.mark_ttl_pending(tbl, now_ms);
      return;
    }
    if let Err(err) = self.maybe_clean_expired(tbl, now_ms).await {
      // 清理是写入提交后的维护动作：失败不回滚、不影响本次写入结果，
      // 但必须可见（含 Corrupted），不能静默丢弃。
      log::warn!("TTL 内联清理失败（写入已提交） tbl={tbl} now_ms={now_ms} err={err}");
    }
  }

  /// 在 `[begin, end)` 内取首条 TTL 条目的过期时间；区间为空返回 `None`。
  ///
  /// 命中上限后的二次探测（是否还有过期项）与未命中后的下一条过期时间
  /// 复用同一份扫描逻辑，避免两处边界条件漂移。
  ///
  /// 坏 TTL key（长度不足 8 字节）不参与定界：把它当 `expire_ts = 0` 会让
  /// `next_expire_ts` 归零、清理永久停摆。这里跳过该条继续向后探测，跳过的
  /// key 收进 `bad_key_li` 由调用方删除：主扫描只覆盖 `[空, expired_end)`，
  /// 出界的坏 key 若只跳过不删除就会永久留在 TTL 树里，每轮探测都为它多读
  /// 一页（r7a-core-05）。连续跳过 [`TTL_BAD_KEY_SKIP_MAX`] 条即放弃本轮。
  ///
  /// `bad_key_li` 的上界是**两阶段累计**的：`clean_expired` 命中扫描上界时会
  /// 先后用同一个 `Vec` 探测「是否还有过期项」与「下一条到期时间」，两段各自
  /// 可能跳过坏 key，合计达到 [`TTL_BAD_KEY_SKIP_MAX`] 即返回
  /// [`TtlPeek::Truncated`]。所以上界不是单次探测的条数，早先一段已跳过的
  /// 坏 key 会让后一段提前截断——这是有意为之：本批坏 key 已收进
  /// `bad_key_li`，下一轮会删掉它们并从新位置继续。
  async fn peek_ttl_expire(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    begin: &[u8],
    end: &[u8],
    bad_key_li: &mut Vec<Vec<u8>>,
  ) -> Result<TtlPeek> {
    let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
    let page_opts = PageOption {
      page_entries: 1,
      ..PageOption::default()
    };
    // 跳过坏 key 后 `begin` 严格前进；再加上跳过计数上界，两者共同保证终止。
    let mut begin = begin.to_vec();
    loop {
      let page = scan_page_with_budget(
        &self.io,
        ScanArg {
          tbl,
          snapshot,
          tree: ScanTree::Ttl,
          begin: &begin,
          end,
          page_size: self.page_size,
          max_entries: 1,
          dict: state.compression.as_ref(),
          index_cache: state.index_cache.as_ref(),
          index_flight: Some(&self.index_flight),
          ttl_filter_now_ms: None,
        },
        ScanOption::default(),
        page_opts,
        self.yield_budget_us,
      )
      .await?;
      let Some(entry) = page.entry_li.first() else {
        return Ok(TtlPeek::Empty);
      };
      match parse_ttl_key(&entry.key) {
        Ok((expire_ts, _)) => return Ok(TtlPeek::Expire(expire_ts)),
        Err(_) => {
          bad_key_li.push(entry.key.clone());
          if bad_key_li.len() >= TTL_BAD_KEY_SKIP_MAX {
            return Ok(TtlPeek::Truncated);
          }
          // `key + 0` 是紧邻其后继的最小 key。
          begin.clone_from(&entry.key);
          begin.push(0);
        }
      }
    }
  }

  /// 探测 `expired_end` 之后的首个到期时间；无更多条目返回 `0`。
  ///
  /// 坏 key 长尾命中跳过上界时压回 `now`：本轮已删掉一批坏 key，下一轮从
  /// 新位置继续，直到长尾清空。
  async fn peek_ttl_after(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    expired_end: &[u8],
    now_ms: u64,
    bad_key_li: &mut Vec<Vec<u8>>,
  ) -> Result<u64> {
    Ok(
      match self
        .peek_ttl_expire(tbl, snapshot, expired_end, &[], bad_key_li)
        .await?
      {
        TtlPeek::Expire(expire_ts) => expire_ts,
        TtlPeek::Empty => 0,
        TtlPeek::Truncated => now_ms.max(1),
      },
    )
  }

  /// 清理 TTL tree 中 `expire_ts <= now_ms` 的条目，并删除对应主树 entry。
  ///
  /// 只有主树 entry 的 `expire_ts` 与 TTL key 一致时才删除，避免误删被覆盖的新值。
  /// 单次最多处理 [`CLEAN_EXPIRED_MAX_ENTRIES`] 条，未清完时返回的 `next_expire_ts`
  /// 为 `now_ms`，由下一轮清理继续。
  pub async fn clean_expired(&mut self, tbl: &TableIdent, now_ms: u64) -> Result<CleanExpiredStat> {
    let Some(snapshot) = self.snapshot(tbl) else {
      return Ok(CleanExpiredStat::default());
    };
    let root_id = snapshot.root_id();
    if root_id == NULL_PAGE {
      self.record_next_expire(tbl, 0);
      return Ok(CleanExpiredStat::default());
    }
    let ttl_root_id = snapshot.ttl_root_id();
    if ttl_root_id == NULL_PAGE {
      self.record_next_expire(tbl, 0);
      return Ok(CleanExpiredStat::default());
    }
    // TTL key = `expire_ts(BE) ++ user_key`，所有 `expire_ts <= now` 的条目都
    // 落在 `[空, ttl_key(now+1))`，无需扫整棵 TTL 树（对齐 C++ CleanExpiredKeys）。
    // 扫描与逐条主树复核共用同一快照，整轮清理看到一致的映射。
    let expired_end = ttl_key(now_ms.saturating_add(1), &[]);
    let entry_li = self
      .scan_expired_entries(tbl, &snapshot, &expired_end)
      .await?;
    // 探测阶段跳过的坏 key 收进 `bad_key_li`，本轮一并删除（r7a-core-05）。
    let mut bad_key_li = Vec::new();
    let next_expire_ts = self
      .probe_next_expire(
        tbl,
        &snapshot,
        &entry_li,
        &expired_end,
        now_ms,
        &mut bad_key_li,
      )
      .await?;
    let (data_delete_li, ttl_delete_li) = self
      .collect_expired_deletes(tbl, &snapshot, entry_li, now_ms, bad_key_li)
      .await?;
    // 读阶段结束：释放快照，后续批次改写当前 mapper。
    drop(snapshot);
    if ttl_delete_li.is_empty() {
      self.record_next_expire(tbl, next_expire_ts);
      return Ok(CleanExpiredStat {
        deleted: 0,
        ttl_deleted: 0,
        next_expire_ts,
      });
    }
    // data 树与 TTL 树两次 write_batch 作为一个批次提交（见
    // `apply_expired_deletes`：任一次失败都回滚 mapper 分配）。
    self
      .apply_expired_deletes(tbl, root_id, ttl_root_id, &data_delete_li, &ttl_delete_li)
      .await?;
    // 批次已结算、读快照已释放：此刻回收本批 retire 的物理页（pooled-04）。
    // 放在这里而非 `put` 路径，纯 TTL 维护负载不必等下一次写才把空间还回池。
    self.reclaim_retired_now(tbl);
    self.record_next_expire(tbl, next_expire_ts);
    Ok(CleanExpiredStat {
      deleted: data_delete_li.len(),
      ttl_deleted: ttl_delete_li.len(),
      next_expire_ts,
    })
  }

  /// 扫描 TTL 树 `[空, expired_end)` 区间，单次最多 [`CLEAN_EXPIRED_MAX_ENTRIES`] 条。
  async fn scan_expired_entries(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    expired_end: &[u8],
  ) -> Result<Vec<ScannedEntry>> {
    let state = self.table_map.get(tbl).ok_or(KvError::ResourceMissing)?;
    scan_entries(
      &self.io,
      ScanArg {
        tbl,
        snapshot,
        tree: ScanTree::Ttl,
        begin: &[],
        end: expired_end,
        page_size: self.page_size,
        max_entries: CLEAN_EXPIRED_MAX_ENTRIES,
        dict: state.compression.as_ref(),
        index_cache: state.index_cache.as_ref(),
        index_flight: Some(&self.index_flight),
        ttl_filter_now_ms: None,
      },
    )
    .await
  }

  /// 探测 `expired_end` 之后的下一到期时间；命中扫描上界时先确认是否还有过期项。
  ///
  /// 扫满上界说明可能还有过期项留在 TTL 树里：把 next_expire 压到当前时刻，
  /// 让写路径 / drain 的下一次调用继续清理，而不是错误地跳到上界之外的下一条。
  async fn probe_next_expire(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    entry_li: &[ScannedEntry],
    expired_end: &[u8],
    now_ms: u64,
    bad_key_li: &mut Vec<Vec<u8>>,
  ) -> Result<u64> {
    let hit_limit = entry_li.len() >= CLEAN_EXPIRED_MAX_ENTRIES;
    // 恰好扫满不等于还有剩余：从最后一条之后的 key 二次探测。确无剩余时
    // 与未扫满路径一样 peek 一条未过期 TTL 维护 next_expire。
    if hit_limit {
      let mut probe_begin = entry_li
        .last()
        .map(|entry| entry.key.clone())
        .ok_or(KvError::Corrupted)?;
      probe_begin.push(0);
      match self
        .peek_ttl_expire(tbl, snapshot, &probe_begin, expired_end, bad_key_li)
        .await?
      {
        // 还有过期项（或坏 key 长尾）：下一轮继续清理。
        TtlPeek::Expire(_) | TtlPeek::Truncated => Ok(now_ms.max(1)),
        TtlPeek::Empty => {
          self
            .peek_ttl_after(tbl, snapshot, expired_end, now_ms, bad_key_li)
            .await
        }
      }
    } else {
      self
        .peek_ttl_after(tbl, snapshot, expired_end, now_ms, bad_key_li)
        .await
    }
  }

  /// 主树复核：逐条确认主树 entry 的 `expire_ts` 与 TTL key 一致后才删除。
  ///
  /// 探测阶段跳过的坏 key 落在主扫描上界之外，只跳过不删除就会永久留在
  /// TTL 树里，这里与过期项同批物理删除，逐轮推进直到长尾清空（r7a-core-05）。
  async fn collect_expired_deletes(
    &self,
    tbl: &TableIdent,
    snapshot: &MappingSnapshot,
    entry_li: Vec<ScannedEntry>,
    now_ms: u64,
    bad_key_li: Vec<Vec<u8>>,
  ) -> Result<(Vec<BatchEntry>, Vec<BatchEntry>)> {
    let mut data_delete_li = Vec::new();
    let mut ttl_delete_li = Vec::new();
    let mut yield_budget = YieldBudget::new(self.yield_budget_us);
    for (seq, entry) in entry_li.into_iter().enumerate() {
      // 每条过期项要做一次主树点读复核，按步长检查段预算。
      yield_budget.maybe_yield_every(seq).await;
      let Ok((expire_ts, user_key)) = parse_ttl_key(&entry.key) else {
        // 坏 TTL key 直接清掉，避免永久占空间。
        // TTL 索引项由引擎写入、版本号恒为 0，墙钟即可压过。
        ttl_delete_li.push(BatchEntry::delete(entry.key, expired_delete_ts(0)));
        continue;
      };
      if expire_ts > now_ms {
        continue;
      }
      if let Some(data_entry) = read_entry(
        &self.io,
        tbl,
        user_key,
        self.page_size,
        // 清理路径需要拿到过期项本身，不能下推过滤。
        self.read_ctx(tbl, snapshot),
      )
      .await?
        && data_entry.expire_ts == expire_ts
      {
        // 删除时间戳必须压过被删条目的版本号：用户 ts 可能是 µs / ns 量级，
        // 直接用毫秒 `now_ms` 会让 delete 在折叠中落败、过期数据永不物理删除。
        data_delete_li.push(BatchEntry::delete(
          user_key.to_vec(),
          expired_delete_ts(data_entry.timestamp),
        ));
      }
      ttl_delete_li.push(BatchEntry::delete(entry.key, expired_delete_ts(0)));
    }
    // 探测阶段跳过的坏 key 落在主扫描上界之外，只跳过不删除就会永久留在
    // TTL 树里，每轮探测都为它多读一页。这里与过期项同批物理删除，逐轮推进
    // 直到长尾清空（r7a-core-05）。
    ttl_delete_li.extend(
      bad_key_li
        .into_iter()
        .map(|key| BatchEntry::delete(key, expired_delete_ts(0))),
    );
    Ok((data_delete_li, ttl_delete_li))
  }

  /// 批量删除 data / TTL 条目：两次 `write_batch` 同批提交，manifest 落盘成功
  /// 后才失效索引缓存并推进表根。
  ///
  /// data 树的写批**不**带 `now_ms`：本批只删除扫描结果里的过期条目。C++
  /// `CleanExpiredKeys` 无扫描上界、整批覆盖所有过期项，其 `add_to_page` 过期
  /// 过滤对本批而言是冗余的；fkv 的 `CLEAN_EXPIRED_MAX_ENTRIES` 是内存上界，
  /// 若在这里也按 `now_ms` 丢弃未扫描到的过期条目，会提前移除主树条目、留下
  /// 孤儿 TTL 索引项，并破坏「上界外的条目本轮不删除」的分批推进语义。
  async fn apply_expired_deletes(
    &mut self,
    tbl: &TableIdent,
    root_id: PageId,
    ttl_root_id: PageId,
    data_delete_li: &[BatchEntry],
    ttl_delete_li: &[BatchEntry],
  ) -> Result<()> {
    // data 树与 TTL 树两次 write_batch 作为一个批次：任一次失败都回滚
    // mapper 分配，避免 OOM 重试时半修改状态。
    // 提交推迟到 manifest 落盘成功之后，失败时 `abort_batch` 才能真正回滚（core-25）。
    let batch = self.begin_mapper_batch(tbl);
    // 写缓存与 `put_at_inner` 同源：本路径也会 COW 重写 TTL / data 树，读到
    // 上一批写出的页同样应命中；若改走裸 IO，`remap_page` 的池复用会绕过缓存
    // 回填，留下陈旧条目。
    let write_cache_bytes = self.write_cache_bytes;
    let roots_result = {
      let state = self
        .table_map
        .get_mut(tbl)
        .ok_or(KvError::ResourceMissing)?;
      let write_cache = state
        .write_cache
        .get_or_insert_with(|| RefCell::new(WritePageCache::new(write_cache_bytes)));
      let mapper = state.mapper.as_mut().ok_or(KvError::ResourceMissing)?;
      let mut segment_mapper = state.segment_mapper.as_mut();
      let dict = state.compression.as_ref();
      let segment_size = self
        .large_value_layout
        .map_or(0, |layout| layout.segment_size());
      let io = CachedWriteIo::new(&self.io, write_cache, self.page_size);
      let result: Result<(PageId, PageId)> = async {
        let new_root = if data_delete_li.is_empty() {
          root_id
        } else {
          write_batch(
            &io,
            WriteArg {
              tbl,
              root_id,
              mapper,
              batch_li: data_delete_li,
              layout: PageLayout::new(
                self.page_size,
                self.data_restart_interval,
                self.index_restart_interval,
                dict,
                self.overflow_pointers,
              ),
              segment_mapper: reborrow_segment(&mut segment_mapper),
              segment_size,
              ttl_delete_sink: None,
            },
          )
          .await?
        };
        let new_ttl_root = write_batch(
          &io,
          WriteArg {
            tbl,
            root_id: ttl_root_id,
            mapper,
            batch_li: ttl_delete_li,
            layout: PageLayout::new(
              self.page_size,
              self.data_restart_interval,
              self.index_restart_interval,
              dict,
              self.overflow_pointers,
            ),
            segment_mapper: reborrow_segment(&mut segment_mapper),
            segment_size,
            ttl_delete_sink: None,
          },
        )
        .await?;
        Ok((new_root, new_ttl_root))
      }
      .await;
      result
    };
    let (new_root, new_ttl_root) = match roots_result {
      Ok(roots) => roots,
      Err(err) => {
        self.settle_mapper_batch(tbl, batch, false);
        return Err(err);
      }
    };
    let log_result = async {
      self.io.sync_data(tbl).await?;
      self.append_mapping_log(tbl, new_root, new_ttl_root).await
    }
    .await;
    self.settle_mapper_batch(tbl, batch, log_result.is_ok());
    log_result?;
    // manifest 成功后再失效：失败时缓存保持原代际，不会出现回滚后 baseline 悬空。
    self.invalidate_index_cache(tbl);
    if let Some(state) = self.table_map.get_mut(tbl) {
      state.root = Some(new_root);
      state.ttl_root = new_ttl_root;
    }
    Ok(())
  }
}
