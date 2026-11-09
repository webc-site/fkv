//! 写路径页缓存（`Store::with_page_cache_bytes` / `Store::page_cache_stat`）。
//!
//! 覆盖任务的三条验收：
//! 1. COW 路径的磁盘页读真的被缓存吃掉（同一 workload 开 / 关缓存对比读次数）；
//! 2. 开 / 关缓存产出**逐字节相同**的 store 目录（含 manifest），读到的永远是最新值；
//! 3. 批次失败（回滚）后不留陈旧缓存条目，重试从干净状态开始。

use std::{cell::Cell, collections::BTreeMap, fs, path::Path, time::Instant};

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
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
    let mut with_cache = Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16)
      .with_page_cache_bytes(CACHE_BYTES)
      .with_enable_data_page_cache(true);
    let mut without_cache =
      Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16).with_page_cache_bytes(CACHE_OFF);
    run_single_key_writes(&mut with_cache, rows).await;
    run_single_key_writes(&mut without_cache, rows).await;

    let on = with_cache.page_io().read_num.get();
    let off = without_cache.page_io().read_num.get();
    let stat = with_cache.page_cache_stat();
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
      Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16).with_page_cache_bytes(CACHE_OFF);
    run_single_key_writes(&mut cached, rows).await;
    let stat = cached.page_cache_stat();
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
  compio_store_data_cache(root, cache_bytes, alloc_mode, true)
}

/// 同上，但显式指定 `enable_data_page_cache`（差分用）。
fn compio_store_data_cache(
  root: &Path,
  cache_bytes: usize,
  alloc_mode: AllocMode,
  data_page_cache: bool,
) -> Store<CompioStore> {
  let layout = SegmentLayout::new(PAGE_SIZE as u32, 2).unwrap();
  Store::new(
    CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT),
    PAGE_SIZE,
    16,
  )
  .with_alloc_mode(alloc_mode)
  .with_large_value(layout, 1024)
  .with_page_cache_bytes(cache_bytes)
  .with_enable_data_page_cache(data_page_cache)
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
      let stat = on.page_cache_stat();
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

/// `enable_data_page_cache` 只影响字节来源，不改变盘面：同一 workload 在
/// 开关 on / off 下产出逐字节相同的 store 目录。
///
/// 与 `write_cache_on_off_store_bytes_identical` 的区别是这里两侧页缓存预算
/// 相同（`CACHE_BYTES`），只切 data 页缓存开关，隔离出本开关的唯一变量。
#[compio::test]
async fn data_page_cache_on_off_store_bytes_identical() {
  for alloc_mode in [AllocMode::Append, AllocMode::Pooled] {
    let on_root = TempRoot::new("dpc_diff_on");
    let off_root = TempRoot::new("dpc_diff_off");
    let mut on = compio_store_data_cache(on_root.path(), CACHE_BYTES, alloc_mode, true);
    let mut off = compio_store_data_cache(off_root.path(), CACHE_BYTES, alloc_mode, false);
    let on_observed = mixed_workload(&mut on).await;
    let off_observed = mixed_workload(&mut off).await;
    assert_eq!(
      on_observed, off_observed,
      "data 页缓存开关不得改变可见数据 (mode={alloc_mode:?})"
    );

    let on_files = dir_snapshot(on_root.path());
    let off_files = dir_snapshot(off_root.path());
    assert_eq!(
      on_files.keys().collect::<Vec<_>>(),
      off_files.keys().collect::<Vec<_>>(),
      "文件集必须一致 (mode={alloc_mode:?})"
    );
    for (name, bytes) in &on_files {
      assert_eq!(
        bytes,
        off_files.get(name).unwrap(),
        "文件字节必须一致: {name} (mode={alloc_mode:?})"
      );
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
      .with_page_cache_bytes(CACHE_BYTES)
      .with_enable_data_page_cache(true);
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
    let stat = store.page_cache_stat();
    println!(
      "pooled reuse: cache_hit={} cache_miss={} cache_len={} bytes={} page_read={}",
      stat.hit,
      stat.miss,
      stat.len,
      stat.bytes,
      store.page_io().read_num.get()
    );
    assert!(stat.hit > 0, "写缓存必须命中");
    // 读写共用缓存后，点读也会命中写路径刚回填的页，因此这里不再有回源读；
    // `page_cache_stat` 已合并读写两侧命中。
    assert_eq!(
      store.page_io().read_num.get(),
      0,
      "写路径回填 + 读路径命中后不应再有回源页读"
    );
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
          return Err(KvError::IoFail);
        }
        self.inner.write_page(tbl, fp, src).await
      }
      async fn write_page_li(
        &self,
        tbl: &TableIdent,
        page_li: &[(FilePageId, Vec<u8>)],
      ) -> Result<()> {
        if self.fail.get() {
          return Err(KvError::IoFail);
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
    .with_page_cache_bytes(CACHE_BYTES)
    .with_enable_data_page_cache(true);
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
    let warm = store.page_cache_stat();
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
    assert_eq!(err, KvError::IoFail);
    let after = store.page_cache_stat();
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
        Err(KvError::IoFail)
      }
      async fn write_page_li(
        &self,
        _tbl: &TableIdent,
        _page_li: &[(FilePageId, Vec<u8>)],
      ) -> Result<()> {
        Err(KvError::IoFail)
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
    .with_page_cache_bytes(CACHE_BYTES)
    .with_enable_data_page_cache(true);
    for _ in 0..3 {
      let err = store
        .put(
          &tbl,
          &[BatchEntry::upsert(b"k".to_vec(), vec![b'v'; 120], 1)],
        )
        .await
        .unwrap_err();
      assert_eq!(err, KvError::IoFail);
      let stat = store.page_cache_stat();
      assert_eq!(stat.len, 0);
      assert_eq!(stat.bytes, 0);
    }
  });
}

// 读路径命中

/// 读缓存把重复读的磁盘页读降到 0：第一遍读填充缓存，后续点读 / 范围扫描 /
/// 全表计数全部命中，读写两侧命中合并进 `page_cache_stat`。
#[test]
fn read_cache_serves_repeated_reads_without_disk_reads() {
  pollster::block_on(async {
    let tbl = TableIdent::new("rc_hit", 1).unwrap();
    let mut store = Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16)
      .with_page_cache_bytes(CACHE_BYTES)
      .with_enable_data_page_cache(true);
    let key_li: Vec<Vec<u8>> = (0..300u32)
      .map(|idx| format!("k{idx:05}").into_bytes())
      .collect();
    let value = |idx: u32| -> Vec<u8> { vec![(idx % 251) as u8; 120] };
    for (round, chunk) in key_li.chunks(50).enumerate() {
      let batch: Vec<BatchEntry> = chunk
        .iter()
        .enumerate()
        .map(|(offset, key)| {
          let idx = round as u32 * 50 + offset as u32;
          BatchEntry::upsert(key.clone(), value(idx), u64::from(idx) + 1)
        })
        .collect();
      store.put(&tbl, &batch).await.unwrap();
    }

    // reopen 会按契约清空页缓存（写路径回填的页全部作废）：第一遍读因此必须
    // 真的回源磁盘，正好用来证明「读缓存确实在起作用」，而不是只靠写路径回填。
    store.reopen(&tbl).await.unwrap();

    // 第一遍读：填充页缓存（读路径回填），必然有回源页读。
    for (idx, key) in key_li.iter().enumerate() {
      assert_eq!(
        store.get(&tbl, key).await.unwrap(),
        Some(value(idx as u32)),
        "第一遍读必须返回最新值"
      );
    }
    let after_first = store.page_io().read_num.get();
    assert!(after_first > 0, "第一遍读必须真的走过磁盘页读");

    // 后续两遍点读：全部命中读缓存，不应再触盘。
    for _ in 0..2 {
      for (idx, key) in key_li.iter().enumerate() {
        assert_eq!(store.get(&tbl, key).await.unwrap(), Some(value(idx as u32)));
      }
    }
    assert_eq!(
      store.page_io().read_num.get(),
      after_first,
      "重复点读必须全部命中页缓存，不得再回源磁盘"
    );

    // 扫描 / 计数也命中同一份页缓存（`read_page_span` 的段尾投机页可能越过
    // 已分配区，按契约不回填，因此允许少量回源；命中数必须确实增长）。
    let hit_before_scan = store.page_cache_stat().hit;
    for _ in 0..2 {
      let scan_li = store.scan(&tbl, b"", b"", 0).await.unwrap();
      assert_eq!(scan_li.len(), key_li.len());
      assert_eq!(
        store.scan_count(&tbl, b"", b"").await.unwrap(),
        key_li.len()
      );
    }
    assert!(
      store.page_cache_stat().hit > hit_before_scan,
      "扫描 / 计数必须命中页缓存"
    );

    let stat = store.page_cache_stat();
    assert!(stat.hit > 0, "读路径必须有命中");
    assert!(stat.miss > 0, "第一遍读必须留下未命中记录");
    assert!(stat.hit > stat.miss);
    assert!(stat.bytes <= CACHE_BYTES, "页缓存常驻不得超预算");
    assert_eq!(stat.capacity, CACHE_BYTES);
    println!(
      "read_cache hit={} miss={} len={} bytes={} page_read={}",
      stat.hit, stat.miss, stat.len, stat.bytes, after_first
    );
  });
}

/// 预算 `0`：读路径完全直通，读结果照常正确，且不产生任何读缓存账目。
#[test]
fn read_cache_disabled_is_a_passthrough() {
  pollster::block_on(async {
    let tbl = TableIdent::new("rc_off", 1).unwrap();
    let mut store =
      Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16).with_page_cache_bytes(CACHE_OFF);
    for round in 0..4u32 {
      let batch: Vec<BatchEntry> = (0..16u32)
        .map(|idx| {
          BatchEntry::upsert(
            format!("k{idx:03}").into_bytes(),
            vec![round as u8 + idx as u8; 100],
            u64::from(round) * 100 + u64::from(idx) + 1,
          )
        })
        .collect();
      store.put(&tbl, &batch).await.unwrap();
    }
    for idx in 0..16u32 {
      assert_eq!(
        store
          .get(&tbl, format!("k{idx:03}").as_bytes())
          .await
          .unwrap(),
        Some(vec![3u8 + idx as u8; 100])
      );
    }
    assert_eq!(store.scan_count(&tbl, b"", b"").await.unwrap(), 16);
    let read = store.page_cache_stat();
    assert_eq!(read.capacity, 0);
    assert_eq!(read.len, 0);
    assert_eq!(read.bytes, 0);
    assert_eq!(read.hit, 0, "关闭时读路径不应有命中");
    assert_eq!(read.miss, 0, "关闭时读路径不应记录未命中");
  });
}

/// 默认 `enable_data_page_cache == false`：data 页读取直读盘、不回填、不计账；
/// index 页照常进缓存并计入命中 / 未命中。
///
/// 构造一棵「leaf index -> 多个 data 页」的树，`reopen` 清空页缓存后：
/// 第一次点读只把 index 页读进缓存（data 页直读）；后续反复点读全部命中
/// index 页，但 data 页仍逐次读盘。`miss` / `len` 不随 data 页读变化，
/// 而 `hit` / 底层页读次数照常增长。
#[test]
fn data_page_cache_off_keeps_data_pages_out_of_cache() {
  pollster::block_on(async {
    let tbl = TableIdent::new("dpc_off", 1).unwrap();
    let mut store = Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16)
      .with_page_cache_bytes(CACHE_BYTES)
      .with_enable_data_page_cache(false);
    assert!(!store.enable_data_page_cache(), "默认必须关闭 data 页缓存");
    let key_li: Vec<Vec<u8>> = (0..600u32)
      .map(|idx| format!("k{idx:05}").into_bytes())
      .collect();
    let value = |idx: u32| -> Vec<u8> { vec![(idx % 251) as u8; 120] };
    for (round, chunk) in key_li.chunks(100).enumerate() {
      let batch: Vec<BatchEntry> = chunk
        .iter()
        .enumerate()
        .map(|(offset, key)| {
          let idx = round as u32 * 100 + offset as u32;
          BatchEntry::upsert(key.clone(), value(idx), u64::from(idx) + 1)
        })
        .collect();
      store.put(&tbl, &batch).await.unwrap();
    }
    // reopen 按契约清空页缓存（含写路径回填的 index 页），保证下面第一遍读
    // 必须真的回源。
    store.reopen(&tbl).await.unwrap();
    // `clear` 只清条目、不重置累计计数器；用读前基线取增量。
    let base = store.page_cache_stat();
    assert_eq!(base.len, 0, "reopen 必须清空缓存条目");

    // 第一次点读：读 index 页（miss + 回填）与 data 页（直读，不记账）。
    assert_eq!(
      store.get(&tbl, &key_li[0]).await.unwrap(),
      Some(value(0)),
      "数据页直读必须返回正确值"
    );
    let after_warm = store.page_cache_stat();
    let reads_after_warm = store.page_io().read_num.get();
    assert!(after_warm.len >= 1, "index 页必须进缓存");
    assert_eq!(
      after_warm.miss - base.miss,
      1,
      "只有 index 页的 miss 被记账"
    );

    // 重复点读 20 个不同 data 页两遍：index 页全命中，data 页每次直读。
    for _ in 0..2 {
      for key in key_li.iter().take(20) {
        store.get(&tbl, key).await.unwrap().expect("key 必须存在");
      }
    }
    let after = store.page_cache_stat();
    let reads_after = store.page_io().read_num.get();
    assert_eq!(
      after.len, after_warm.len,
      "data 页读取不得增加缓存条目: warm={} after={}",
      after_warm.len, after.len
    );
    assert_eq!(after.miss, after_warm.miss, "data 页读取不得增加 miss 计数");
    assert!(
      after.hit > after_warm.hit,
      "index 页命中必须照常计数: {} -> {}",
      after_warm.hit,
      after.hit
    );
    assert!(
      reads_after > reads_after_warm,
      "data 页关闭缓存后必须每次回源读盘: {reads_after_warm} -> {reads_after}"
    );
  });
}

/// 打开 `enable_data_page_cache`：行为与改造前一致——index 页与 data 页都进
/// 缓存，首次读的 index / data 各记一次 miss，重复读全部命中且不再触盘。
#[test]
fn data_page_cache_on_caches_index_and_data_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("dpc_on", 1).unwrap();
    let mut store = Store::new(CountingIo::new(PAGE_SIZE), PAGE_SIZE, 16)
      .with_page_cache_bytes(CACHE_BYTES)
      .with_enable_data_page_cache(true);
    let key_li: Vec<Vec<u8>> = (0..600u32)
      .map(|idx| format!("k{idx:05}").into_bytes())
      .collect();
    let value = |idx: u32| -> Vec<u8> { vec![(idx % 251) as u8; 120] };
    for (round, chunk) in key_li.chunks(100).enumerate() {
      let batch: Vec<BatchEntry> = chunk
        .iter()
        .enumerate()
        .map(|(offset, key)| {
          let idx = round as u32 * 100 + offset as u32;
          BatchEntry::upsert(key.clone(), value(idx), u64::from(idx) + 1)
        })
        .collect();
      store.put(&tbl, &batch).await.unwrap();
    }
    store.reopen(&tbl).await.unwrap();

    // 首次点读：index 页 + data 页各回源一次、各记一次 miss。
    let before = store.page_cache_stat();
    assert_eq!(store.get(&tbl, &key_li[0]).await.unwrap(), Some(value(0)));
    let warmed = store.page_cache_stat();
    assert!(
      warmed.miss - before.miss >= 2,
      "index + data 页首次读都必须记 miss: {} -> {}",
      before.miss,
      warmed.miss
    );
    assert!(warmed.len >= 2, "index 与 data 页都必须进缓存");
    let reads_warm = store.page_io().read_num.get();

    // 同一 key 重复读：index + data 全命中，不再触盘。
    assert_eq!(store.get(&tbl, &key_li[0]).await.unwrap(), Some(value(0)));
    let hit_after_repeat = store.page_cache_stat();
    assert!(
      hit_after_repeat.hit - warmed.hit >= 2,
      "index + data 页重复读都必须命中: {} -> {}",
      warmed.hit,
      hit_after_repeat.hit
    );
    assert_eq!(
      store.page_io().read_num.get(),
      reads_warm,
      "重复读同一 key 不得再触盘"
    );

    // 20 个不同 key 读两遍：第一遍可能补读 data 页，第二遍必须全部命中。
    for key in key_li.iter().take(20) {
      store.get(&tbl, key).await.unwrap().expect("key 必须存在");
    }
    let reads_after_first_pass = store.page_io().read_num.get();
    for key in key_li.iter().take(20) {
      store.get(&tbl, key).await.unwrap().expect("key 必须存在");
    }
    assert_eq!(
      store.page_io().read_num.get(),
      reads_after_first_pass,
      "第二遍读必须全部命中页缓存"
    );
  });
}

// 读结果差分：缓存开 / 关逐条一致

/// 同一 op log 在缓存开 / 关两侧的**读结果逐条一致**：覆盖 TTL 过期过滤
/// （同一 key 在 now 早于 / 晚于 `expire_ts` 两侧）、删除、覆盖与大 value
/// 拒绝口径（`get` 对大 value 返回 `LargeValueUnsupported`）。
async fn read_observation_log(store: &mut Store<CompioStore>) -> Vec<String> {
  let tbl = TableIdent::new("rc_diff", 1).unwrap();
  let mut obs = Vec::new();
  let value = |seed: u32| -> Vec<u8> { vec![(seed % 251) as u8; 150] };
  let mut ts = 1u64;
  for round in 0..4u32 {
    let batch: Vec<BatchEntry> = (0..8u32)
      .map(|idx| {
        let key = format!("k{:03}", round * 8 + idx).into_bytes();
        BatchEntry::upsert(key, value(round * 8 + idx), ts)
      })
      .collect();
    ts += 1;
    store.put(&tbl, &batch).await.unwrap();
  }
  // 覆盖一半 + 删除四分之一。
  let batch: Vec<BatchEntry> = (0..16u32)
    .map(|idx| {
      let key = format!("k{idx:03}").into_bytes();
      BatchEntry::upsert(key, value(100 + idx), ts)
    })
    .collect();
  ts += 1;
  store.put(&tbl, &batch).await.unwrap();
  let batch: Vec<BatchEntry> = (24..32u32)
    .map(|idx| BatchEntry::delete(format!("k{idx:03}").into_bytes(), ts))
    .collect();
  ts += 1;
  store.put(&tbl, &batch).await.unwrap();
  // TTL：`ttl` 在 now=900 存活、now=1100 过期（读侧过滤）。
  let mut ttl = BatchEntry::upsert(b"ttl".to_vec(), value(7), ts);
  ttl.expire_ts = 1_000;
  ts += 1;
  store.put(&tbl, &[ttl]).await.unwrap();
  // 大 value：`get` 必须返回 `LargeValueUnsupported`（与缓存开关无关）。
  store
    .put_large(
      &tbl,
      &[BatchEntry::upsert(b"big".to_vec(), vec![0xAB; 9000], ts)],
    )
    .await
    .unwrap();

  for now in [900u64, 1_100] {
    for key in (0..32u32)
      .map(|idx| format!("k{idx:03}").into_bytes())
      .chain([b"ttl".to_vec(), b"big".to_vec(), b"missing".to_vec()])
    {
      obs.push(format!(
        "get {key:?} @{now} = {:?}",
        store.get_at(&tbl, &key, now).await
      ));
    }
    obs.push(format!(
      "scan @{now} = {:?}",
      store.scan_entry_at(&tbl, b"", b"", 0, now).await
    ));
    obs.push(format!(
      "scan_count @{now} = {:?}",
      store.scan_count_at(&tbl, b"", b"", now).await
    ));
    obs.push(format!(
      "floor k010 = {:?}",
      store.floor(&tbl, b"k010").await
    ));
  }
  obs
}

/// 缓存开 / 关产出逐条相同的读观测与逐字节相同的盘面。
#[compio::test]
async fn read_cache_on_off_read_results_and_bytes_identical() {
  for alloc_mode in [AllocMode::Append, AllocMode::Pooled] {
    for (cache_bytes, tag) in [(CACHE_BYTES, "4MiB"), (CACHE_TINY, "8KiB")] {
      let on_root = TempRoot::new("rc_diff_on");
      let off_root = TempRoot::new("rc_diff_off");
      let mut on = compio_store(on_root.path(), cache_bytes, alloc_mode);
      let mut off = compio_store(off_root.path(), CACHE_OFF, alloc_mode);
      let on_obs = read_observation_log(&mut on).await;
      let off_obs = read_observation_log(&mut off).await;
      assert_eq!(
        on_obs, off_obs,
        "读缓存不得改变任何读结果 (mode={alloc_mode:?} cache={tag})"
      );
      assert!(
        on_obs
          .iter()
          .any(|line| line.contains("LargeValueUnsupported")),
        "大 value 拒绝口径必须被观测到"
      );
      assert!(
        on_obs.iter().any(|line| line.contains("None")),
        "删除 / 过期口径必须被观测到"
      );
      let read = on.page_cache_stat();
      assert!(read.hit > 0, "mode={alloc_mode:?} cache={tag} 应有读命中");
      println!(
        "mode={alloc_mode:?} cache={tag} read_hit={} read_miss={} len={} bytes={}",
        read.hit, read.miss, read.len, read.bytes
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

// 变更后读：写 / 删 / 截断 / reopen / compaction

/// 写后读、删除后读、compaction 后读、reopen 后读、truncate 后读都返回最新值。
#[compio::test]
async fn read_cache_sees_writes_deletes_compaction_reopen_and_truncate() {
  let root = TempRoot::new("pc_mut");
  let tbl = TableIdent::new("pc_mut", 1).unwrap();
  let layout = SegmentLayout::new(PAGE_SIZE as u32, 2).unwrap();
  let mut store = Store::new(
    CompioStore::new(
      root.path().to_path_buf(),
      PAGE_SIZE,
      DEFAULT_PAGES_PER_FILE_SHIFT,
    ),
    PAGE_SIZE,
    16,
  )
  .with_alloc_mode(AllocMode::Append)
  .with_large_value(layout, 1024)
  .with_file_amplify_factor(1)
  .with_page_cache_bytes(CACHE_BYTES)
  .with_enable_data_page_cache(true);
  let key_li: Vec<Vec<u8>> = (0..64u32)
    .map(|idx| format!("k{idx:04}").into_bytes())
    .collect();
  let value = |round: u32, idx: u32| -> Vec<u8> { vec![(round * 37 + idx) as u8; 200] };

  // 写后立即读。
  let batch: Vec<BatchEntry> = key_li
    .iter()
    .enumerate()
    .map(|(idx, key)| BatchEntry::upsert(key.clone(), value(0, idx as u32), idx as u64 + 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  for (idx, key) in key_li.iter().enumerate() {
    assert_eq!(
      store.get(&tbl, key).await.unwrap(),
      Some(value(0, idx as u32)),
      "写后读"
    );
  }

  // 覆盖后读（COW 换物理页，旧页进 freed 表）。
  let batch: Vec<BatchEntry> = key_li
    .iter()
    .enumerate()
    .map(|(idx, key)| BatchEntry::upsert(key.clone(), value(1, idx as u32), 100 + idx as u64))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  for (idx, key) in key_li.iter().enumerate() {
    assert_eq!(
      store.get(&tbl, key).await.unwrap(),
      Some(value(1, idx as u32)),
      "覆盖后读"
    );
  }

  // 删除后读。
  let batch: Vec<BatchEntry> = key_li[32..]
    .iter()
    .enumerate()
    .map(|(idx, key)| BatchEntry::delete(key.clone(), 200 + idx as u64))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  for (idx, key) in key_li.iter().enumerate() {
    let expect = (idx < 32).then(|| value(1, idx as u32));
    assert_eq!(store.get(&tbl, key).await.unwrap(), expect, "删除后读");
  }

  // compaction 后读（搬页走裸 IO，缓存整体作废后按未命中重建）。
  let moved = store.compact_if_needed(&tbl).await.unwrap();
  println!("compact moved file num = {}", moved.len());
  for (idx, key) in key_li.iter().enumerate() {
    let expect = (idx < 32).then(|| value(1, idx as u32));
    assert_eq!(
      store.get(&tbl, key).await.unwrap(),
      expect,
      "compaction 后读"
    );
  }

  // reopen 后读（mapper 重建成新化身，缓存清空）。
  store.reopen(&tbl).await.unwrap();
  for (idx, key) in key_li.iter().enumerate() {
    let expect = (idx < 32).then(|| value(1, idx as u32));
    assert_eq!(store.get(&tbl, key).await.unwrap(), expect, "reopen 后读");
  }

  // truncate 后读（整表页释放，不得命中任何旧页）。
  store.truncate(&tbl).await.unwrap();
  for key in &key_li {
    assert_eq!(store.get(&tbl, key).await.unwrap(), None, "truncate 后读");
  }
  assert_eq!(store.scan_count(&tbl, b"", b"").await.unwrap(), 0);

  // 截断后重写：新页复用旧物理页号也必须是新字节。
  let batch: Vec<BatchEntry> = key_li[..8]
    .iter()
    .enumerate()
    .map(|(idx, key)| BatchEntry::upsert(key.clone(), value(2, idx as u32), 300 + idx as u64))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  for (idx, key) in key_li[..8].iter().enumerate() {
    assert_eq!(
      store.get(&tbl, key).await.unwrap(),
      Some(value(2, idx as u32)),
      "truncate 后重写读"
    );
  }
  for key in &key_li[8..] {
    assert_eq!(store.get(&tbl, key).await.unwrap(), None);
  }
  let read = store.page_cache_stat();
  assert!(read.hit > 0 && read.miss > 0);
  assert!(read.bytes <= CACHE_BYTES);
}

/// test-23：compaction 开批前整体清空页缓存（`compact.rs` 入口的
/// `clear_page_cache`）的锁定测试。搬页走裸 IO 不经写路径装饰器，Pooled
/// 下新物理页可能落在缓存里仍有旧条目的 id 上——删掉该清空调用会出现
/// 「快照回填退役页 → maintain 回池 → compact 裸写复用同 id」的陈旧读
/// 窗口（第 32 轮语义域探针实证 load-bearing），而现有套件对该调用零
/// 判别力（删掉后全绿）。本用例锁「compact 后缓存占用必须归零 + 数据仍
/// 可读」：占用断言在正确性回读之前，避免回读自身回填干扰。
#[compio::test]
async fn compact_clears_page_cache_before_moving_pages() {
  const TINY_PAGE: usize = 512;
  /// 8 页 / 文件：60 × 100B 值摊到多文件，file 0 制造空洞后成为压缩候选。
  const TINY_SHIFT: u8 = 3;
  let tbl = TableIdent::new("demo", 1).unwrap();
  let mut store = Store::new(
    MemStore::with_pages_per_file(TINY_PAGE, TINY_SHIFT),
    TINY_PAGE,
    16,
  )
  .with_page_cache_bytes(CACHE_BYTES)
  .with_enable_data_page_cache(true);

  let batch: Vec<BatchEntry> = (0..60u32)
    .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'v'; 100], 1))
    .collect();
  store.put(&tbl, &batch).await.unwrap();
  // 删掉 file 0 前段 key：该文件变部分 live，成为可压缩候选。
  let dead: Vec<BatchEntry> = (0..10u32)
    .map(|i| BatchEntry::delete(format!("k{i:03}").into_bytes(), 1))
    .collect();
  store.put(&tbl, &dead).await.unwrap();
  // 读热缓存（file 0 与后续文件的页都进缓存）——前提锁定：占用非零。
  for i in 0..60u32 {
    store.read(&tbl, format!("k{i:03}").as_bytes()).await.ok();
  }
  assert!(store.page_cache_stat().bytes > 0, "夹具必须先把页缓存读热");

  let moved = store.compact_file(&tbl, 0, TINY_SHIFT).await.unwrap();
  assert!(moved > 0, "夹具必须真实搬页（moved={moved}）");
  let stat = store.page_cache_stat();
  assert_eq!(
    (stat.bytes, stat.len),
    (0, 0),
    "compact 开批前必须整体清空页缓存：删掉 clear_page_cache 即红"
  );
  // 清空不破坏正确性：幸存 key 全部可读（回读会重新回填缓存，故在占用
  // 断言之后执行）。
  for i in 10..60u32 {
    let value = store.read(&tbl, format!("k{i:03}").as_bytes()).await;
    assert_eq!(value.unwrap(), vec![b'v'; 100], "k{i:03} 应可读");
  }
}
