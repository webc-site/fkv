//! 写路径页缓存（`Store::with_write_cache_bytes` / `Store::write_cache_stat`）。
//!
//! 覆盖任务的三条验收：
//! 1. COW 路径的磁盘页读真的被缓存吃掉（同一 workload 开 / 关缓存对比读次数）；
//! 2. 开 / 关缓存产出**逐字节相同**的 store 目录（含 manifest），读到的永远是最新值；
//! 3. 批次失败（回滚）后不留陈旧缓存条目，重试从干净状态开始。

use std::{cell::Cell, collections::BTreeMap, fs, path::Path, time::Instant};

use fkv_btree::BatchEntry;
use fkv_error::Result;
use fkv_io::{ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_large::SegmentLayout;
use fkv_mapping::AllocMode;
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, FilePageId, TableIdent};

const PAGE_SIZE: usize = 4096;
const CACHE_BYTES: usize = 4 * 1024 * 1024;
/// 只够几页的写缓存：强制路径上的 Clock 逐出 / 释放失效反复发生。
const CACHE_TINY: usize = 8 * 1024;
/// 预算 `0` = 显式关闭写缓存（差分对照组）。
const CACHE_OFF: usize = 0;

// Mem 代理 IO

/// 计数 `read_page*` 次数的代理 IO：验证缓存把盘读变成命中。
struct CountingIo {
  inner: MemStore,
  read_num: Cell<u64>,
}

impl CountingIo {
  fn new(page_size: usize) -> Self {
    Self {
      inner: MemStore::new(page_size),
      read_num: Cell::new(0),
    }
  }

  fn bump(&self, num: u64) {
    self.read_num.set(self.read_num.get() + num);
  }
}

impl PageIo for CountingIo {
  async fn read_page(&self, tbl: &TableIdent, fp: FilePageId, dst: &mut [u8]) -> Result<()> {
    self.bump(1);
    self.inner.read_page(tbl, fp, dst).await
  }

  async fn read_page_owned(
    &self,
    tbl: &TableIdent,
    fp: FilePageId,
    buf: &mut Vec<u8>,
  ) -> Result<()> {
    self.bump(1);
    self.inner.read_page_owned(tbl, fp, buf).await
  }

  async fn read_page_li(
    &self,
    tbl: &TableIdent,
    fp_li: &[FilePageId],
    buf_li: &mut [Vec<u8>],
  ) -> Result<()> {
    self.bump(fp_li.len() as u64);
    self.inner.read_page_li(tbl, fp_li, buf_li).await
  }

  async fn write_page(&self, tbl: &TableIdent, fp: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, fp, src).await
  }

  async fn write_page_li(&self, tbl: &TableIdent, page_li: &[(FilePageId, Vec<u8>)]) -> Result<()> {
    self.inner.write_page_li(tbl, page_li).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl ManifestIo for CountingIo {
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

impl SegmentIo for CountingIo {
  async fn read_segment(&self, tbl: &TableIdent, fp: FilePageId, dst: &mut [u8]) -> Result<()> {
    self.inner.read_segment(tbl, fp, dst).await
  }

  async fn write_segment(&self, tbl: &TableIdent, fp: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_segment(tbl, fp, src).await
  }
}

/// 单键批序列：`rows` 次 `put`（每次 1 条 upsert），key 升序、value 128B。
async fn run_single_key_writes<I>(store: &mut Store<I>, rows: u32)
where
  I: PageIo + ManifestIo,
{
  let tbl = TableIdent::new("wc", 1).unwrap();
  let value = vec![b'v'; 128];
  let started = Instant::now();
  for idx in 0..rows {
    let key = format!("k{idx:08}").into_bytes();
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(key, value.clone(), idx as u64 + 1)],
      )
      .await
      .unwrap();
  }
  let us = started.elapsed().as_secs_f64() * 1e6 / f64::from(rows);
  // 纯 CPU 下界（无 fsync / manifest 开销）：与 compio bench 的 put_avg_us 相减
  // 可分解「CPU vs 每批 fsync IO」。
  println!("rows={rows} memstore_cpu_put_avg_us={us:.2}");
}

/// 验收 1：同一单键批 workload，开缓存后写路径磁盘页读大幅下降。
#[test]
fn write_cache_cuts_cow_path_disk_reads() {
  pollster::block_on(async {
    let rows = 20_000u32;
    let mut with_cache =
      Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16).with_write_cache_bytes(CACHE_BYTES);
    let mut without_cache =
      Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16).with_write_cache_bytes(CACHE_OFF);
    run_single_key_writes(&mut with_cache, rows).await;
    run_single_key_writes(&mut without_cache, rows).await;

    let on = with_cache.page_io().read_num.get();
    let off = without_cache.page_io().read_num.get();
    let stat = with_cache.write_cache_stat();
    println!(
      "rows={rows} page_read_on={on} ({:.3}/put) page_read_off={off} ({:.3}/put) \
       cache_hit={} cache_miss={} cache_len={}",
      on as f64 / f64::from(rows),
      off as f64 / f64::from(rows),
      stat.hit,
      stat.miss,
      stat.len,
    );
    assert!(
      on * 3 < off,
      "写缓存必须吃掉大部分 COW 路径页读: on={on} off={off}"
    );
    assert!(stat.hit > 0, "写缓存必须产生命中");
    assert_eq!(stat.capacity, CACHE_BYTES);
    // 缓存常驻不超过预算。
    assert!(stat.bytes <= CACHE_BYTES);
  });
}

/// 关闭缓存时装饰器完全直通：不给 io 增加任何读。
#[test]
fn disabled_cache_is_a_passthrough() {
  pollster::block_on(async {
    let rows = 2_000u32;
    let mut cached =
      Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16).with_write_cache_bytes(CACHE_OFF);
    run_single_key_writes(&mut cached, rows).await;
    let stat = cached.write_cache_stat();
    assert_eq!(stat.capacity, 0);
    assert_eq!(stat.len, 0);
    assert_eq!(stat.bytes, 0);
    assert_eq!(stat.hit, 0, "关闭时不应有命中");
    assert_eq!(stat.miss, 0, "关闭时不应记录未命中");
    assert!(cached.page_io().read_num.get() > 0);
  });
}

// 差分：盘面逐字节

/// `root` 下全部文件的 `(相对路径, 字节)`，按路径排序。
fn dir_snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
  fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
      let path = entry.unwrap().path();
      if path.is_dir() {
        walk(root, &path, out);
      } else {
        let rel = path
          .strip_prefix(root)
          .unwrap()
          .to_string_lossy()
          .into_owned();
        out.insert(rel, fs::read(&path).unwrap());
      }
    }
  }
  let mut out = BTreeMap::new();
  walk(root, root, &mut out);
  out
}

fn compio_store(root: &Path, cache_bytes: usize, alloc_mode: AllocMode) -> Store<CompioStore> {
  let layout = SegmentLayout::new(PAGE_SIZE as u32, 2).unwrap();
  Store::new(
    CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT),
    PAGE_SIZE,
    16,
  )
  .with_alloc_mode(alloc_mode)
  .with_large_value(layout, 1024)
  .with_write_cache_bytes(cache_bytes)
}

/// 混合 workload：批量 put、覆盖、删除、大 value、交错点读。
///
/// 返回该 workload 读到的 `(key, value)` 观测记录，供两侧逐条比对（证明缓存从不
/// 影响可见数据）。
async fn mixed_workload(store: &mut Store<CompioStore>) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
  let tbl = TableIdent::new("diff", 1).unwrap();
  let mut observed = Vec::new();
  let value = |seed: usize| -> Vec<u8> {
    (0..200)
      .map(|idx| (idx * 7 + seed) as u8)
      .collect::<Vec<u8>>()
  };
  let big = |seed: usize| -> Vec<u8> { vec![seed as u8; 9000] };
  let mut ts = 1u64;
  for round in 0..6u32 {
    // 批量 upsert（8 条一批）。
    let batch: Vec<BatchEntry> = (0..8)
      .map(|idx| {
        let key = format!("k{:03}", round * 8 + idx).into_bytes();
        BatchEntry::upsert(key, value((round * 8 + idx) as usize), ts)
      })
      .collect();
    ts += 1;
    store.put(&tbl, &batch).await.unwrap();
    // 覆盖其中一半（值更长，触发页内重排 / 溢出候选）。
    let batch: Vec<BatchEntry> = (0..4)
      .map(|idx| {
        let key = format!("k{:03}", round * 8 + idx).into_bytes();
        BatchEntry::upsert(key, value(100 + (round * 4 + idx) as usize), ts)
      })
      .collect();
    ts += 1;
    store.put(&tbl, &batch).await.unwrap();
    // 大 value（走 segment 文件）。
    let big_key = format!("big{round:03}").into_bytes();
    store
      .put_large(
        &tbl,
        &[BatchEntry::upsert(big_key.clone(), big(round as usize), ts)],
      )
      .await
      .unwrap();
    ts += 1;
    // 删除一半小 key。
    let batch: Vec<BatchEntry> = (0..4)
      .map(|idx| {
        let key = format!("k{:03}", round * 8 + 4 + idx).into_bytes();
        BatchEntry::delete(key, ts)
      })
      .collect();
    ts += 1;
    store.put(&tbl, &batch).await.unwrap();
    // 交错点读：每个 key 读回，记录观测（含大 value 与已删 key）。
    for key_idx in 0..32u32 {
      let key = format!("k{key_idx:03}").into_bytes();
      observed.push((key.clone(), store.get(&tbl, &key).await.unwrap()));
    }
    observed.push((
      big_key.clone(),
      Some(
        store
          .get_large(&tbl, &big_key)
          .await
          .unwrap()
          .unwrap()
          .value,
      ),
    ));
  }
  observed
}

/// 验收 2：缓存开 / 关产出逐字节相同的 store 目录，且读到的最新值一致。
///
/// 两种预算都跑：`CACHE_BYTES`（装得下工作集）与 `CACHE_TINY`（只够几页，强制
/// Clock 逐出 / 反复失效），后者顺带证明「逐出后回源重读」不改变任何可见字节。
#[compio::test]
async fn write_cache_on_off_store_bytes_identical() {
  for alloc_mode in [AllocMode::Append, AllocMode::Pooled] {
    for (cache_bytes, tag) in [(CACHE_BYTES, "4MiB"), (CACHE_TINY, "8KiB")] {
      let on_root = TempRoot::new("wc_diff_on");
      let off_root = TempRoot::new("wc_diff_off");
      let mut on = compio_store(on_root.path(), cache_bytes, alloc_mode);
      let mut off = compio_store(off_root.path(), CACHE_OFF, alloc_mode);
      let on_observed = mixed_workload(&mut on).await;
      let off_observed = mixed_workload(&mut off).await;
      assert_eq!(
        on_observed, off_observed,
        "缓存不得改变可见数据 (mode={alloc_mode:?} cache={tag})"
      );
      assert!(
        on_observed.iter().any(|(_, value)| value.is_some()),
        "workload 必须真的写到数据"
      );
      let stat = on.write_cache_stat();
      assert!(
        stat.hit > 0,
        "mode={alloc_mode:?} cache={tag} 应有写缓存命中"
      );
      assert!(stat.bytes <= cache_bytes, "缓存常驻不得超预算");
      println!(
        "mode={alloc_mode:?} cache={tag} hit={} miss={} evict={} len={} bytes={}",
        stat.hit, stat.miss, stat.evict, stat.len, stat.bytes
      );

      let on_files = dir_snapshot(on_root.path());
      let off_files = dir_snapshot(off_root.path());
      assert_eq!(
        on_files.keys().collect::<Vec<_>>(),
        off_files.keys().collect::<Vec<_>>(),
        "文件集必须一致 (mode={alloc_mode:?} cache={tag})"
      );
      for (name, bytes) in &on_files {
        assert_eq!(
          bytes,
          off_files.get(name).unwrap(),
          "文件字节必须一致: {name} (mode={alloc_mode:?} cache={tag})"
        );
      }
    }
  }
}

// 池复用正确性

/// 验收 3（端到端）：Pooled 下反复覆盖 / 删除（物理页会被回收再复用）后，
/// 每个 key 都读到最新值，且写缓存真的在命中。
#[test]
fn write_cache_pooled_page_reuse_keeps_latest_values() {
  pollster::block_on(async {
    let tbl = TableIdent::new("reuse", 1).unwrap();
    let mut store = Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16)
      .with_alloc_mode(AllocMode::Pooled)
      .with_write_cache_bytes(CACHE_BYTES);
    let key_num = 400u32;
    for round in 0..8u32 {
      // 全量覆盖（COW remap + retire），值随轮次变化。
      let batch: Vec<BatchEntry> = (0..key_num)
        .map(|idx| {
          BatchEntry::upsert(
            format!("k{idx:05}").into_bytes(),
            vec![(round as u8).wrapping_add(idx as u8); 100],
            u64::from(round) * 100 + u64::from(idx) + 1,
          )
        })
        .collect();
      store.put(&tbl, &batch).await.unwrap();
      // 隔轮删除后半段，制造空闲页 → 池复用。
      if round % 2 == 1 {
        let batch: Vec<BatchEntry> = (key_num / 2..key_num)
          .map(|idx| {
            BatchEntry::delete(
              format!("k{idx:05}").into_bytes(),
              u64::from(round) * 100 + u64::from(idx) + 50,
            )
          })
          .collect();
        store.put(&tbl, &batch).await.unwrap();
      }
      // 逐 key 校验最新值。
      for idx in 0..key_num {
        let key = format!("k{idx:05}").into_bytes();
        let expect = if round % 2 == 1 && idx >= key_num / 2 {
          None
        } else {
          Some(vec![(round as u8).wrapping_add(idx as u8); 100])
        };
        assert_eq!(
          store.get(&tbl, &key).await.unwrap(),
          expect,
          "round={round} key={idx}"
        );
      }
    }
    let stat = store.write_cache_stat();
    println!(
      "pooled reuse: cache_hit={} cache_miss={} cache_len={} bytes={} page_read={}",
      stat.hit,
      stat.miss,
      stat.len,
      stat.bytes,
      store.page_io().read_num.get()
    );
    assert!(stat.hit > 0, "写缓存必须命中");
    assert!(store.page_io().read_num.get() > 0, "对照：仍有回源读");
  });
}

// 回滚

/// 失败批次（`write_page_li` 报错）必须整体清空写缓存：回滚后不得留下把
/// 本批已落盘字节当成活跃映射的条目。
#[test]
fn failed_batch_clears_write_cache() {
  pollster::block_on(async {
    struct FailingIo {
      inner: MemStore,
      fail: Cell<bool>,
    }
    impl PageIo for FailingIo {
      async fn read_page(&self, tbl: &TableIdent, fp: FilePageId, dst: &mut [u8]) -> Result<()> {
        self.inner.read_page(tbl, fp, dst).await
      }
      async fn read_page_owned(
        &self,
        tbl: &TableIdent,
        fp: FilePageId,
        buf: &mut Vec<u8>,
      ) -> Result<()> {
        self.inner.read_page_owned(tbl, fp, buf).await
      }
      async fn write_page(&self, tbl: &TableIdent, fp: FilePageId, src: &[u8]) -> Result<()> {
        if self.fail.get() {
          return Err(fkv_error::KvError::IoFail);
        }
        self.inner.write_page(tbl, fp, src).await
      }
      async fn write_page_li(
        &self,
        tbl: &TableIdent,
        page_li: &[(FilePageId, Vec<u8>)],
      ) -> Result<()> {
        if self.fail.get() {
          return Err(fkv_error::KvError::IoFail);
        }
        self.inner.write_page_li(tbl, page_li).await
      }
      async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
        self.inner.sync_data(tbl).await
      }
    }
    impl ManifestIo for FailingIo {
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
      async fn write_manifest_file(
        &self,
        tbl: &TableIdent,
        name: &str,
        record: &[u8],
      ) -> Result<()> {
        self.inner.write_manifest_file(tbl, name, record).await
      }
      async fn append_manifest_file(
        &self,
        tbl: &TableIdent,
        name: &str,
        record: &[u8],
      ) -> Result<()> {
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

    let tbl = TableIdent::new("rollback", 1).unwrap();
    let mut store = Store::new(
      FailingIo {
        inner: MemStore::new(PAGE_SIZE),
        fail: Cell::new(false),
      },
      PAGE_SIZE,
      16,
    )
    .with_write_cache_bytes(CACHE_BYTES);
    // 先写几批把缓存捂热。
    for round in 0..6u32 {
      let batch: Vec<BatchEntry> = (0..8)
        .map(|idx| {
          BatchEntry::upsert(
            format!("k{idx:03}").into_bytes(),
            vec![round as u8 + idx as u8; 120],
            u64::from(round) + 1,
          )
        })
        .collect();
      store.put(&tbl, &batch).await.unwrap();
    }
    let warm = store.write_cache_stat();
    assert!(warm.hit > 0 && warm.len > 0, "预热后应有缓存条目");

    // 注入失败：本批写页报错 → `abort_batch` 回滚，写缓存必须被清空。
    store.page_io().fail.set(true);
    let err = store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"k000".to_vec(), vec![b'x'; 120], 999)],
      )
      .await
      .unwrap_err();
    assert_eq!(err, fkv_error::KvError::IoFail);
    let after = store.write_cache_stat();
    assert_eq!(after.len, 0, "回滚后不得留下缓存条目");
    assert_eq!(after.bytes, 0, "回滚后不得留下缓存字节");

    // 恢复后重试成功，且旧数据仍在（回滚把映射恢复到批次前）。
    store.page_io().fail.set(false);
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"k000".to_vec(), vec![b'y'; 120], 1000),
          BatchEntry::upsert(b"k999".to_vec(), vec![b'z'; 120], 1000),
        ],
      )
      .await
      .unwrap();
    assert_eq!(
      store.get(&tbl, b"k000").await.unwrap(),
      Some(vec![b'y'; 120])
    );
    assert_eq!(
      store.get(&tbl, b"k999").await.unwrap(),
      Some(vec![b'z'; 120])
    );
    assert_eq!(
      store.get(&tbl, b"k001").await.unwrap(),
      Some(vec![6u8; 120])
    );
  });
}

/// 变体：只要 io 的写一直失败，重复重试也不得把缓存条目攒回来。
#[test]
fn repeated_failed_batches_keep_cache_empty() {
  pollster::block_on(async {
    struct AlwaysFailWrite {
      inner: MemStore,
    }
    impl PageIo for AlwaysFailWrite {
      async fn read_page(&self, tbl: &TableIdent, fp: FilePageId, dst: &mut [u8]) -> Result<()> {
        self.inner.read_page(tbl, fp, dst).await
      }
      async fn read_page_owned(
        &self,
        tbl: &TableIdent,
        fp: FilePageId,
        buf: &mut Vec<u8>,
      ) -> Result<()> {
        self.inner.read_page_owned(tbl, fp, buf).await
      }
      async fn write_page(&self, _tbl: &TableIdent, _fp: FilePageId, _src: &[u8]) -> Result<()> {
        Err(fkv_error::KvError::IoFail)
      }
      async fn write_page_li(
        &self,
        _tbl: &TableIdent,
        _page_li: &[(FilePageId, Vec<u8>)],
      ) -> Result<()> {
        Err(fkv_error::KvError::IoFail)
      }
      async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
        self.inner.sync_data(tbl).await
      }
    }
    impl ManifestIo for AlwaysFailWrite {
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
      async fn write_manifest_file(
        &self,
        tbl: &TableIdent,
        name: &str,
        record: &[u8],
      ) -> Result<()> {
        self.inner.write_manifest_file(tbl, name, record).await
      }
      async fn append_manifest_file(
        &self,
        tbl: &TableIdent,
        name: &str,
        record: &[u8],
      ) -> Result<()> {
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

    let tbl = TableIdent::new("rollback2", 1).unwrap();
    let mut store = Store::new(
      AlwaysFailWrite {
        inner: MemStore::new(PAGE_SIZE),
      },
      PAGE_SIZE,
      16,
    )
    .with_write_cache_bytes(CACHE_BYTES);
    for _ in 0..3 {
      let err = store
        .put(
          &tbl,
          &[BatchEntry::upsert(b"k".to_vec(), vec![b'v'; 120], 1)],
        )
        .await
        .unwrap_err();
      assert_eq!(err, fkv_error::KvError::IoFail);
      let stat = store.write_cache_stat();
      assert_eq!(stat.len, 0);
      assert_eq!(stat.bytes, 0);
    }
  });
}
