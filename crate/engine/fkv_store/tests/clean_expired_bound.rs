//! perf-86：内联 TTL 清理失败要可见，且单次清理按上界分批。

use std::{
  cell::Cell,
  sync::{Mutex, Once},
  thread::sleep,
  time::Duration,
};

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_store::{CLEAN_EXPIRED_MAX_ENTRIES, CleanExpiredStat, Store};
use fkv_types::{FilePageId, PageId, TableIdent, now_ms};
use log::{Level, LevelFilter};

static LOG_LI: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LOG_INIT: Once = Once::new();

struct CaptureLogger;

impl log::Log for CaptureLogger {
  fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
    metadata.level() <= Level::Warn
  }

  fn log(&self, record: &log::Record<'_>) {
    if self.enabled(record.metadata()) {
      LOG_LI.lock().unwrap().push(record.args().to_string());
    }
  }

  fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger;

fn init_log() {
  LOG_INIT.call_once(|| {
    log::set_logger(&LOGGER).expect("测试日志器只注册一次");
    log::set_max_level(LevelFilter::Warn);
  });
}

/// 指定逻辑页读取返回 `Corrupted`，其余操作透传 `MemStore`。
struct FlakyStore {
  inner: MemStore,
  fail_fp: Cell<Option<FilePageId>>,
  /// 剩余允许成功的 `sync_data` 次数；为 0 时下一次失败。
  fail_sync_after: Cell<Option<usize>>,
}

impl FlakyStore {
  fn new(page_size: usize) -> Self {
    Self {
      inner: MemStore::new(page_size),
      fail_fp: Cell::new(None),
      fail_sync_after: Cell::new(None),
    }
  }

  fn new_with_segments(page_size: usize, segment_size: usize, shift: u8) -> Self {
    Self {
      inner: MemStore::new(page_size).with_segment_config(segment_size, shift),
      fail_fp: Cell::new(None),
      fail_sync_after: Cell::new(None),
    }
  }

  fn fail_on(&self, file_page_id: FilePageId) {
    self.fail_fp.set(Some(file_page_id));
  }

  fn clear_fail(&self) {
    self.fail_fp.set(None);
    self.fail_sync_after.set(None);
  }

  /// 跳过 `skip` 次成功后让下一次 `sync_data` 返回 `Corrupted`。
  fn fail_sync_after(&self, skip: usize) {
    self.fail_sync_after.set(Some(skip));
  }
}

impl PageIo for FlakyStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    if self.fail_fp.get() == Some(file_page_id) {
      return Err(KvError::Corrupted);
    }
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    if let Some(skip) = self.fail_sync_after.get() {
      if skip == 0 {
        self.fail_sync_after.set(None);
        return Err(KvError::Corrupted);
      }
      self.fail_sync_after.set(Some(skip - 1));
    }
    self.inner.sync_data(tbl).await
  }
}

impl SegmentIo for FlakyStore {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

impl ManifestIo for FlakyStore {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.append_manifest(tbl, record).await
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.inner.switch_manifest(tbl, record).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest(tbl).await
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.drop_manifest(tbl).await
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.inner.write_manifest_file(tbl, name, record).await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.inner.append_manifest_file(tbl, name, record).await
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.inner.list_manifest_file_name_li(tbl).await
  }

  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.inner.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.inner.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    self.inner.read_manifest_file(tbl, name).await
  }

  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> Result<()> {
    self.inner.seed_local_manifest_file(tbl, name, bytes).await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.inner.remove_manifest_file(tbl, name).await
  }
}

fn upsert_with_expire(key: &[u8], value: &[u8], ts: u64, expire_ts: u64) -> BatchEntry {
  let mut entry = BatchEntry::upsert(key.to_vec(), value.to_vec(), ts);
  entry.expire_ts = expire_ts;
  entry
}

#[test]
fn inline_clean_error_is_warned_and_put_still_succeeds() {
  pollster::block_on(async {
    init_log();
    let tbl = TableIdent::new("clean_warn", 1).unwrap();
    let mut store = Store::new(FlakyStore::new(512), 512, 16);
    // 过期项放首张 data 页，足够多的 live key 撑出多页：后续写只碰最后一页，
    // 写批不会内联丢弃 a（sem-50），清理失败语义才能独立验证。
    let mut batch = vec![upsert_with_expire(b"a", b"A", 1, 100)];
    for index in 0..300u32 {
      batch.push(BatchEntry::upsert(
        format!("k{index:03}").into_bytes(),
        b"v".to_vec(),
        1,
      ));
    }
    store.put_at(&tbl, &batch, 0).await.unwrap();

    // 只让 TTL 树根读取失败：put 提交后的内联清理在扫描 TTL 树时命中 Corrupted。
    let ttl_root = store.ttl_root_id(&tbl).unwrap();
    assert_ne!(ttl_root, PageId::MAX);
    let ttl_fp = store.mapper(&tbl).unwrap().file_page_id(ttl_root).unwrap();
    LOG_LI.lock().unwrap().clear();
    store.page_io().fail_on(ttl_fp);

    let put_result = store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"zzzz".to_vec(), b"Z".to_vec(), 2)],
        100,
      )
      .await;
    assert!(put_result.is_ok(), "清理失败不能改变写成功语义");
    assert_eq!(
      store.get_at(&tbl, b"zzzz", 0).await.unwrap(),
      Some(b"Z".to_vec()),
      "本次写入必须已提交"
    );
    assert_eq!(
      store.get_at(&tbl, b"a", 0).await.unwrap(),
      Some(b"A".to_vec()),
      "清理失败时过期项应保留到下一轮"
    );

    let log_li = LOG_LI.lock().unwrap().clone();
    assert!(
      log_li.iter().any(|line| line.contains("TTL 内联清理失败")
        && line.contains("clean_warn")
        && line.contains("Disk data corrupted")),
      "缺少可见的 warn 日志: {log_li:?}"
    );

    // 故障解除后 next_expire 仍指向 100，下一次写入继续触发清理且状态未损坏。
    store.page_io().clear_fail();
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"zzzz".to_vec(), b"Z2".to_vec(), 3)],
        100,
      )
      .await
      .unwrap();
    assert_eq!(store.get_at(&tbl, b"a", 0).await.unwrap(), None);
    assert_eq!(
      store.get_at(&tbl, b"zzzz", 0).await.unwrap(),
      Some(b"Z2".to_vec())
    );
  });
}

#[test]
fn clean_expired_is_bounded_and_continues_next_round() {
  pollster::block_on(async {
    let tbl = TableIdent::new("clean_bound", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let total = CLEAN_EXPIRED_MAX_ENTRIES + 3;
    let batch: Vec<BatchEntry> = (0..total)
      .map(|index| upsert_with_expire(format!("e{index:04}").as_bytes(), b"v", 1, 100))
      .collect();
    store.put_at(&tbl, &batch, 0).await.unwrap();

    let first = store.clean_expired(&tbl, 100).await.unwrap();
    assert_eq!(first.deleted, CLEAN_EXPIRED_MAX_ENTRIES);
    assert_eq!(first.ttl_deleted, CLEAN_EXPIRED_MAX_ENTRIES);
    assert_eq!(first.next_expire_ts, 100, "扫满上界后下一轮必须立即继续");
    let last_key = format!("e{:04}", total - 1);
    assert_eq!(
      store.get_at(&tbl, last_key.as_bytes(), 0).await.unwrap(),
      Some(b"v".to_vec()),
      "上界外的条目本轮不能删除"
    );

    let second = store.clean_expired(&tbl, 100).await.unwrap();
    assert_eq!(second.deleted, 3);
    assert_eq!(second.ttl_deleted, 3);
    assert_eq!(second.next_expire_ts, 0);
    for index in 0..total {
      let key = format!("e{index:04}");
      assert_eq!(store.get_at(&tbl, key.as_bytes(), 0).await.unwrap(), None);
    }

    let third = store.clean_expired(&tbl, 100).await.unwrap();
    assert_eq!(third, CleanExpiredStat::default());
  });
}

/// 恰好清满上界时二次探测必须确认“没有剩余”，不能多跑一轮。
#[test]
fn clean_expired_exact_limit_reports_no_more() {
  pollster::block_on(async {
    let tbl = TableIdent::new("clean_exact", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let total = CLEAN_EXPIRED_MAX_ENTRIES;
    let batch: Vec<BatchEntry> = (0..total)
      .map(|index| upsert_with_expire(format!("e{index:04}").as_bytes(), b"v", 1, 100))
      .collect();
    store.put_at(&tbl, &batch, 0).await.unwrap();

    let first = store.clean_expired(&tbl, 100).await.unwrap();
    assert_eq!(first.deleted, total);
    assert_eq!(first.ttl_deleted, total);
    assert_eq!(
      first.next_expire_ts, 0,
      "恰好 256 条清完不应把 next_expire 压到当前时刻"
    );
    for index in 0..total {
      let key = format!("e{index:04}");
      assert_eq!(store.get_at(&tbl, key.as_bytes(), 0).await.unwrap(), None);
    }
  });
}

/// 延迟清理：drain 必须把超过单轮上界的积压一次清完并清掉 pending。
#[test]
fn drain_pending_ttl_clears_backlog_beyond_limit() {
  pollster::block_on(async {
    let tbl = TableIdent::new("clean_drain", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16).with_deferred_ttl(true);
    let total = CLEAN_EXPIRED_MAX_ENTRIES + 5;
    let batch: Vec<BatchEntry> = (0..total)
      .map(|index| upsert_with_expire(format!("e{index:04}").as_bytes(), b"v", 1, 100))
      .collect();
    store.put_at(&tbl, &batch, 0).await.unwrap();

    // 到点后的写只标 pending，不做内联清理。
    store
      .put_at(
        &tbl,
        &[BatchEntry::upsert(b"tail".to_vec(), b"T".to_vec(), 2)],
        100,
      )
      .await
      .unwrap();
    assert_eq!(store.pending_ttl_len(), 1);

    assert_eq!(store.drain_pending_ttl_at(100).await.unwrap(), 1);
    assert_eq!(store.pending_ttl_len(), 0, "积压清完后 pending 必须归零");
    for index in 0..total {
      let key = format!("e{index:04}");
      assert_eq!(store.get_at(&tbl, key.as_bytes(), 0).await.unwrap(), None);
    }
    assert_eq!(
      store.get_at(&tbl, b"tail", 100).await.unwrap(),
      Some(b"T".to_vec())
    );
  });
}

/// put_large 的段 journal 与 TTL 清理不再嵌套：清理阶段落盘失败时必须
/// 连同段释放一起回滚，过期大 value 的段仍可读，不能出现悬垂引用。
#[test]
fn put_large_cleanup_failure_rolls_back_segment_free() {
  pollster::block_on(async {
    init_log();
    let tbl = TableIdent::new("large_ttl_nested", 1).unwrap();
    let layout = SegmentLayout::new(4096, 2).unwrap();
    let mut store =
      Store::new(FlakyStore::new_with_segments(512, 4096, 2), 512, 16).with_large_value(layout, 64);
    let big = vec![b'L'; 9000];
    // 过期时间取近期，等它到点后由 put_large 触发内联清理。big 放首张 data 页，
    // 300 条 live key 撑出多页：第二次写只碰最后一页，不会内联丢弃 big
    // （sem-50），清理阶段的失败与段回滚才能独立验证。
    let expire_ts = now_ms() + 20;
    let mut seed = vec![upsert_with_expire(b"big", &big, 1, expire_ts)];
    for index in 0..300u32 {
      seed.push(BatchEntry::upsert(
        format!("m{index:03}").into_bytes(),
        b"v".to_vec(),
        1,
      ));
    }
    store.put_large(&tbl, &seed).await.unwrap();
    sleep(Duration::from_millis(40));

    // 第一条 sync_data 属于本次写入，第二条属于清理：让清理在写页之后失败。
    LOG_LI.lock().unwrap().clear();
    store.page_io().fail_sync_after(1);
    let result = store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(b"zzzz".to_vec(), b"v2".to_vec(), 2)],
      )
      .await;
    assert!(result.is_ok(), "清理失败不能改变写成功语义");
    assert_eq!(
      store.get_at(&tbl, b"zzzz", 0).await.unwrap(),
      Some(b"v2".to_vec())
    );
    let log_li = LOG_LI.lock().unwrap().clone();
    assert!(
      log_li.iter().any(|line| line.contains("TTL 内联清理失败")),
      "清理失败必须可见: {log_li:?}"
    );

    // 回滚后过期项仍在（now=0 视角未过期），其段映射必须完整可读。
    assert_eq!(
      store
        .get_large_at(&tbl, b"big", 0)
        .await
        .unwrap()
        .map(|v| v.value),
      Some(big),
      "清理回滚必须同时恢复段释放，否则条目悬垂指向已释放段"
    );
  });
}
