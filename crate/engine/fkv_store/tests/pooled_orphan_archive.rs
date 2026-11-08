//! sem-28：孤儿归档（branch manifest 已删 / 部分删除）必须参与 Pooled 复用下界。
//!
//! C++ Local `DeleteBranch` 只删无 tag manifest + CURRENT_TERM，带 tag 归档
//! 原样保留；Rust `delete_branch` 按字典序先删 `manifest_<branch>_<term>`、
//! 再删归档，删归档失败 / 进程崩溃同样留下孤儿归档。修复前
//! `rebuild_reuse_floor` 对「其它 branch 的归档」直接 `continue`，前提是该
//! branch manifest 仍在盘上；孤儿状态下 main 的下界保持 0，Pooled 空闲池把
//! 归档预留块整块回填，main 继续写覆盖归档页——`reopen_archive` 仍成功，但
//! 所有 key 读成 `None`（MemStore 静默丢数据）。
//!
//! 修复后归档 manifest 按自身区间链派生预留块末尾（`branch_block_base +
//! RESERVE`）计入候选；受限分支沿用块起点过滤与上限 clamp（pooled-08）。
//! C++ 风格布局（main 水位低于孤儿块）在 Append 下同样由 `reserve_until`
//! 跳过整个孤儿块。

use std::iter::{Empty, empty};

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_io_mem::MemStore;
use fkv_io_sim::SimStore;
use fkv_large::SegmentLayout;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_store::{AllocMode, BRANCH_FILE_RESERVE_FILE_NUM, Store};
use fkv_testkit::TempRoot;
use fkv_types::{
  BranchFileRange, FileId, FilePageId, NULL_PAGE, TableIdent, decode_id, is_file_page_id,
  parse_manifest_file_name,
};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 8;
/// 4 页 / 文件；预留块 1024 文件 = 4096 页。
const SHIFT: u8 = 2;
const BLOCK_FP: u64 = BRANCH_FILE_RESERVE_FILE_NUM << SHIFT;
/// 1 个 300B 的 key 约 1 页；5000 条足以从空闲池高端降到归档页所在低端。
const FLOOD_BATCHES: u32 = 50;
const FLOOD_ENTRIES: u32 = 100;

fn new_mem() -> MemStore {
  MemStore::with_pages_per_file(PAGE_SIZE, SHIFT)
}

fn mem_store(mode: AllocMode) -> Store<MemStore> {
  reopen(mode, new_mem())
}

fn reopen<I: PageIo + ManifestIo>(mode: AllocMode, io: I) -> Store<I> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(mode)
}

fn compio_store(root: &TempRoot) -> Store<CompioStore> {
  Store::new(
    CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, SHIFT),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(AllocMode::Pooled)
}

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

fn key(prefix: &str, index: u32) -> Vec<u8> {
  format!("{prefix}{index}").into_bytes()
}

fn value(prefix: &str, index: u32) -> Vec<u8> {
  format!("{}{index}", prefix.to_uppercase()).into_bytes()
}

fn floor_of<I: PageIo + ManifestIo>(store: &Store<I>, tbl: &TableIdent) -> u64 {
  store.mapper(tbl).expect("mapper").reuse_floor()
}

fn max_fp_of<I: PageIo + ManifestIo>(store: &Store<I>, tbl: &TableIdent) -> u64 {
  store.mapper(tbl).expect("mapper").max_fp_id()
}

/// main 覆盖 a0，再写入 [`FLOOD_BATCHES`] × [`FLOOD_ENTRIES`] 条 300B key。
async fn flood_main<I>(store: &mut Store<I>, tbl: &TableIdent)
where
  I: PageIo + ManifestIo,
{
  store.put(tbl, &[up(b"a0", b"A0-new", 3)]).await.unwrap();
  let payload = vec![b'X'; 300];
  for batch in 0..FLOOD_BATCHES {
    let entries: Vec<_> = (0..FLOOD_ENTRIES)
      .map(|i| {
        let n = batch * FLOOD_ENTRIES + i;
        BatchEntry::upsert(
          format!("c{n:05}").into_bytes(),
          payload.clone(),
          4 + u64::from(n),
        )
      })
      .collect();
    store.put(tbl, &entries).await.unwrap();
  }
}

/// 归档 manifest 必须仍能读回 a0..a7 / b0..b7 的归档值。
async fn assert_archive<I>(store: &mut Store<I>, tbl: &TableIdent)
where
  I: PageIo + ManifestIo,
{
  store.prepare_attach("f1", 0).unwrap();
  store.reopen_archive(tbl, "keep").await.unwrap();
  for index in 0..8 {
    assert_eq!(
      store.get(tbl, &key("a", index)).await.unwrap(),
      Some(value("a", index)),
      "孤儿归档的继承页被 main 覆盖 a{index}"
    );
    assert_eq!(
      store.get(tbl, &key("b", index)).await.unwrap(),
      Some(value("b", index)),
      "孤儿归档的自身页被 main 覆盖 b{index}"
    );
  }
}

/// main 建 f1 → f1 写 b 并归档 keep → 删除 f1 的 branch manifest。
async fn build_orphan<I>(store: &mut Store<I>, tbl: &TableIdent)
where
  I: PageIo + ManifestIo + FileIo + SegmentIo,
{
  for index in 0..8 {
    store
      .put(tbl, &[up(&key("a", index), &value("a", index), 1)])
      .await
      .unwrap();
  }
  store.create_branch(tbl, "f1").await.unwrap();
}

/// C++ Local `DeleteBranch`：只删无 tag manifest，归档保留。
#[compio::test]
async fn orphan_archive_survives_main_pooled_writes() {
  let tbl = TableIdent::new("p28_orphan", 1).unwrap();
  let mut main = mem_store(AllocMode::Pooled);
  build_orphan(&mut main, &tbl).await;
  let mut f1 = reopen(AllocMode::Pooled, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  for index in 0..8 {
    f1.put(&tbl, &[up(&key("b", index), &value("b", index), 2)])
      .await
      .unwrap();
  }
  f1.create_archive(&tbl, "keep", 0).await.unwrap();
  f1.page_io()
    .remove_manifest_file(&tbl, "manifest_f1_0")
    .await
    .unwrap();

  let mut main = reopen(AllocMode::Pooled, f1.into_io());
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  // Rust 布局：create_branch 把 main 水位推到 f1 预留块末尾 8192 页。
  assert_eq!(max_fp_of(&main, &tbl), 2 * BLOCK_FP);
  assert_eq!(
    floor_of(&main, &tbl),
    2 * BLOCK_FP,
    "孤儿归档的预留块末尾必须计入 main 的下界"
  );

  flood_main(&mut main, &tbl).await;
  assert_eq!(
    floor_of(&main, &tbl),
    2 * BLOCK_FP,
    "main 写入不得把下界重新降回 0"
  );
  assert!(
    max_fp_of(&main, &tbl) > 2 * BLOCK_FP,
    "main 必须从孤儿块末尾之后继续分配"
  );
  assert_archive(&mut reopen(AllocMode::Pooled, main.into_io()), &tbl).await;
}

/// Rust `delete_branch` 在删完 branch manifest 后、删归档前 IO 失败。
#[compio::test]
async fn delete_branch_partial_failure_keeps_archive_reusable() {
  let tbl = TableIdent::new("p28_partial", 1).unwrap();
  let mut main = Store::new(FailArchiveRemove::new(), PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(AllocMode::Pooled);
  build_orphan(&mut main, &tbl).await;
  let mut f1 = reopen(AllocMode::Pooled, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  for index in 0..8 {
    f1.put(&tbl, &[up(&key("b", index), &value("b", index), 2)])
      .await
      .unwrap();
  }
  f1.create_archive(&tbl, "keep", 0).await.unwrap();

  let mut main = reopen(AllocMode::Pooled, f1.into_io());
  assert_eq!(
    main.delete_branch(&tbl, "f1").await,
    Err(KvError::IoFail),
    "删归档必须失败"
  );
  assert_eq!(
    main
      .page_io()
      .list_manifest_file_name_li(&tbl)
      .await
      .unwrap(),
    vec!["manifest_f1_0_keep".to_owned()],
    "部分删除后只剩孤儿归档"
  );

  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(
    floor_of(&main, &tbl),
    2 * BLOCK_FP,
    "delete_branch 部分失败留下的孤儿归档同样必须参与下界"
  );
  flood_main(&mut main, &tbl).await;
  assert_archive(&mut reopen(AllocMode::Pooled, main.into_io()), &tbl).await;
}

/// 控制组：branch manifest 仍在盘上时行为不变（既有池化路径不回归）。
#[compio::test]
async fn active_branch_manifest_archive_still_reusable() {
  let tbl = TableIdent::new("p28_active", 1).unwrap();
  let mut main = mem_store(AllocMode::Pooled);
  build_orphan(&mut main, &tbl).await;
  let mut f1 = reopen(AllocMode::Pooled, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  for index in 0..8 {
    f1.put(&tbl, &[up(&key("b", index), &value("b", index), 2)])
      .await
      .unwrap();
  }
  f1.create_archive(&tbl, "keep", 0).await.unwrap();

  let mut main = reopen(AllocMode::Pooled, f1.into_io());
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(floor_of(&main, &tbl), 2 * BLOCK_FP);
  flood_main(&mut main, &tbl).await;
  assert_archive(&mut reopen(AllocMode::Pooled, main.into_io()), &tbl).await;
}

/// 受限分支（f1）不得把兄弟 f2 的孤儿块当自己的下界；main 则必须计入。
#[compio::test]
async fn limited_branch_ignores_sibling_orphan_archive_block() {
  let tbl = TableIdent::new("p28_sibling", 1).unwrap();
  let mut main = mem_store(AllocMode::Pooled);
  main.put(&tbl, &[up(b"a0", b"A0", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();
  let mut f1 = reopen(AllocMode::Pooled, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
  f1.create_branch(&tbl, "f2").await.unwrap();

  let mut f2 = reopen(AllocMode::Pooled, f1.into_io());
  f2.open_branch(&tbl, "f2", 0).await.unwrap();
  let f2_base = max_fp_of(&f2, &tbl);
  assert_eq!(f2_base % BLOCK_FP, 0, "f2 分支 manifest max 即块基址");
  f2.put(&tbl, &[up(b"b0", b"B0", 3)]).await.unwrap();
  f2.create_archive(&tbl, "keep", 0).await.unwrap();
  f2.page_io()
    .remove_manifest_file(&tbl, "manifest_f2_0")
    .await
    .unwrap();

  // f1 是受限分支：兄弟 f2 的块起点高于自己高水位，不得成为下界
  // （否则水位被推过 branch_file_limit，写批次 flush 后才判 OutOfSpace）。
  let mut f1 = reopen(AllocMode::Pooled, f2.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  let limit = f1.branch_file_limit().expect("f1 必须有预留上限");
  assert!(
    floor_of(&f1, &tbl) < f2_base,
    "受限分支的下界越过了兄弟孤儿块：floor={} f2_base={f2_base}",
    floor_of(&f1, &tbl)
  );
  assert!(floor_of(&f1, &tbl) <= limit);
  assert!(max_fp_of(&f1, &tbl) <= limit);
  f1.put(&tbl, &[up(b"n0", b"N0", 4)]).await.unwrap();
  assert_eq!(f1.get(&tbl, b"n0").await.unwrap(), Some(b"N0".to_vec()));

  // main 能分配到全部子孙块：孤儿 f2 归档块末尾必须计入下界。
  let mut main = reopen(AllocMode::Pooled, f1.into_io());
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(
    floor_of(&main, &tbl),
    f2_base + BLOCK_FP,
    "main 必须按孤儿归档自己的区间链取块末尾"
  );
}

/// Append + C++ 风格布局：main 水位低于孤儿块时，main 续写也不得覆盖归档。
///
/// Rust `create_branch` 会把 main 水位 reserve 到子块末尾，天然避开孤儿块；
/// C++ Local 写的 branch 没有这条预留，main 的 `max_fp_id` 可能停在孤儿块
/// 之前。这里把 main 的活跃 manifest 水位改回 fork 前，复刻该布局。
#[compio::test]
async fn orphan_archive_append_cpp_layout_not_overwritten() {
  let tbl = TableIdent::new("p28_append", 1).unwrap();
  let mut main = mem_store(AllocMode::Append);
  for index in 0..8 {
    main
      .put(&tbl, &[up(&key("a", index), &value("a", index), 1)])
      .await
      .unwrap();
  }
  let main_before = max_fp_of(&main, &tbl);
  assert!(main_before > 0 && main_before < BLOCK_FP);
  main.create_branch(&tbl, "f1").await.unwrap();

  let mut f1 = reopen(AllocMode::Append, main.into_io());
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  for index in 0..8 {
    f1.put(&tbl, &[up(&key("b", index), &value("b", index), 2)])
      .await
      .unwrap();
  }
  f1.create_archive(&tbl, "keep", 0).await.unwrap();
  let io = f1.into_io();
  io.remove_manifest_file(&tbl, "manifest_f1_0")
    .await
    .unwrap();
  lower_main_watermark(&io, &tbl, main_before).await;

  let mut main = reopen(AllocMode::Append, io);
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(
    max_fp_of(&main, &tbl),
    2 * BLOCK_FP,
    "Append main 的分配水位必须跳到孤儿归档块末尾之后"
  );

  flood_main(&mut main, &tbl).await;
  assert_archive(&mut reopen(AllocMode::Append, main.into_io()), &tbl).await;
}

/// 把 main 活跃 manifest 的水位改回 `max_fp_id`（模拟 C++ 未预留子块的布局）。
async fn lower_main_watermark(io: &MemStore, tbl: &TableIdent, max_fp_id: u64) {
  let bytes = io
    .read_manifest(tbl)
    .await
    .unwrap()
    .expect("main 活跃 manifest");
  let (state, _) = fkv_task_reopen::replay_manifest_bytes_with_len(&bytes).unwrap();
  let record = ManifestBuilder::new()
    .snapshot_with(
      SnapshotArg {
        root_id: state.root_id,
        ttl_root: state.ttl_root_id,
        max_fp_id,
        dict_bytes: &state.dict_bytes,
        branch_metadata: &state.branch_metadata,
        max_segment_fp_id: state.max_segment_fp_id,
      },
      state.mapping_tbl.iter_value_li(),
      None::<Empty<u64>>,
    )
    .unwrap();
  io.switch_manifest(tbl, &record).await.unwrap();
}

/// Compio：孤儿归档的 `data_<id>_<branch>_<term>` 对象在 main 写入后仍可读回，
/// 且 main 的 file id 分配不再与归档身份重叠。
#[compio::test]
async fn compio_orphan_archive_survives_main_writes() {
  let root = TempRoot::new("p28_compio");
  let tbl = TableIdent::new("p28_compio", 1).unwrap();
  let mut main = compio_store(&root);
  build_orphan(&mut main, &tbl).await;

  let mut f1 = compio_store(&root);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  for index in 0..8 {
    f1.put(&tbl, &[up(&key("b", index), &value("b", index), 2)])
      .await
      .unwrap();
  }
  f1.create_archive(&tbl, "keep", 0).await.unwrap();
  f1.page_io()
    .remove_manifest_file(&tbl, "manifest_f1_0")
    .await
    .unwrap();

  let mut main = compio_store(&root);
  main.prepare_attach("main", 0).unwrap();
  main.reopen(&tbl).await.unwrap();
  assert_eq!(max_fp_of(&main, &tbl), 2 * BLOCK_FP);
  assert_eq!(
    floor_of(&main, &tbl),
    2 * BLOCK_FP,
    "Compio main 同样不得重新分配孤儿归档块"
  );
  flood_main(&mut main, &tbl).await;

  let mut arch = compio_store(&root);
  assert_archive(&mut arch, &tbl).await;
}

/// `MemStore` 包装：带 tag 的 manifest 删除返回 `IoFail`，其余透传。
///
/// 删除顺序按 `list_manifest_file_name_li` 的字典序：`manifest_f1_0` 先于
/// `manifest_f1_0_keep`，因此 branch manifest 删除成功、归档删除失败，正好
/// 复刻 `delete_branch` 的中途失败窗口。
struct FailArchiveRemove {
  inner: MemStore,
}

impl FailArchiveRemove {
  fn new() -> Self {
    Self { inner: new_mem() }
  }
}

impl PageIo for FailArchiveRemove {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> Result<()> {
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(&self, tbl: &TableIdent, file_page_id: FilePageId, src: &[u8]) -> Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl SegmentIo for FailArchiveRemove {
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

impl FileIo for FailArchiveRemove {
  async fn list_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_file_id_li(tbl).await
  }

  async fn read_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<Vec<u8>>> {
    self.inner.read_file(tbl, file_id).await
  }

  async fn write_file(&self, tbl: &TableIdent, file_id: FileId, bytes: &[u8]) -> Result<()> {
    self.inner.write_file(tbl, file_id, bytes).await
  }

  async fn file_size(&self, tbl: &TableIdent, file_id: FileId) -> Result<Option<u64>> {
    self.inner.file_size(tbl, file_id).await
  }

  async fn remove_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_file(tbl, file_id).await
  }

  async fn list_segment_file_id_li(&self, tbl: &TableIdent) -> Result<Vec<FileId>> {
    self.inner.list_segment_file_id_li(tbl).await
  }

  async fn remove_segment_file(&self, tbl: &TableIdent, file_id: FileId) -> Result<()> {
    self.inner.remove_segment_file(tbl, file_id).await
  }
}

impl ManifestIo for FailArchiveRemove {
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
    let tagged = parse_manifest_file_name(name).is_some_and(|(_, _, tag)| tag.is_some());
    if tagged {
      return Err(KvError::IoFail);
    }
    self.inner.remove_manifest_file(tbl, name).await
  }
}

// pooled-13：生产配置（分区恒配置 large value）下的孤儿归档冷重开
//
// 生产 shard / SDK 无条件 `with_large_value`，于是 `create_branch` 的
// `use_segment` 恒为 true，branch manifest / 归档都带「子分支段块基址」作为
// `max_segment_fp_id`——即使该 branch 从未写过大 value。branch manifest 被
// `delete_branch` 删掉后（sem-28 孤儿归档），冷 Store 的 `reopen_archive`
// 没有活跃段水位，旧判据 `archive_seg_max > 0` 会把纯数据归档误判成「带段
// 体系」而返回 `ResourceMissing`（pooled-13）。修复后按存活 manifest 的段页
// 证据重建保守水位：无段页证据时归档水位（块基址）本身安全，有更晚归档 /
// 子分支引用时抬到引用之上。

const SEG_SIZE: usize = 4096;
/// 4 段 / 段文件，与 data 侧同位移。
const SEG_SHIFT: u8 = 2;
const THRESHOLD: usize = 1024;
/// 3 段：同一段文件内，回退水位即覆盖。
const BIG_LEN: usize = 9000;
const PROD_MODES: [AllocMode; 2] = [AllocMode::Pooled, AllocMode::Append];

fn large_layout() -> SegmentLayout {
  SegmentLayout::new(SEG_SIZE as u32, SEG_SHIFT).expect("segment layout")
}

/// 生产形态：`with_large_value` 恒开启。
fn prod_store<I: PageIo>(io: I, mode: AllocMode) -> Store<I> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_large_value(large_layout(), THRESHOLD)
    .with_alloc_mode(mode)
}

trait ProdBackend {
  type Io: PageIo + ManifestIo + FileIo + SegmentIo;

  fn label(&self) -> &'static str;

  fn open(&self, mode: AllocMode) -> Store<Self::Io>;

  /// 冷进程 / 重启：丢弃内存表状态，IO 承载的盘面保留。
  fn restart(&self, mode: AllocMode, store: Store<Self::Io>) -> Store<Self::Io> {
    prod_store(store.into_io(), mode)
  }
}

struct MemProd;

impl ProdBackend for MemProd {
  type Io = MemStore;

  fn label(&self) -> &'static str {
    "Mem"
  }

  fn open(&self, mode: AllocMode) -> Store<MemStore> {
    prod_store(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT).with_segment_config(SEG_SIZE, SEG_SHIFT),
      mode,
    )
  }
}

struct SimProd;

impl ProdBackend for SimProd {
  type Io = SimStore;

  fn label(&self) -> &'static str {
    "Sim"
  }

  fn open(&self, mode: AllocMode) -> Store<SimStore> {
    prod_store(
      SimStore::new(PAGE_SIZE)
        .with_pages_per_file(PAGE_SIZE, SHIFT)
        .with_segment_config(SEG_SIZE, SEG_SHIFT),
      mode,
    )
  }
}

struct CompioProd<'a> {
  root: &'a TempRoot,
}

impl ProdBackend for CompioProd<'_> {
  type Io = CompioStore;

  fn label(&self) -> &'static str {
    "Compio"
  }

  fn open(&self, mode: AllocMode) -> Store<CompioStore> {
    prod_store(
      CompioStore::new(self.root.path().to_path_buf(), PAGE_SIZE, SHIFT)
        .with_segment_config(SEG_SIZE, SEG_SHIFT),
      mode,
    )
  }
}

fn seg_max<I: PageIo>(store: &Store<I>, tbl: &TableIdent) -> u64 {
  store
    .segment_mapper(tbl)
    .unwrap_or_else(|| panic!("segment mapper 缺失"))
    .max_fp_id()
}

async fn read_large<I>(store: &Store<I>, tbl: &TableIdent, key: &[u8]) -> Option<Vec<u8>>
where
  I: PageIo + SegmentIo,
{
  store
    .get_large(tbl, key)
    .await
    .unwrap()
    .map(|read| read.value)
}

/// `main` 建 f1 → f1 写 b0..b7 → 归档 keep → 删除 f1 的 branch manifest。
async fn build_prod_orphan<H: ProdBackend>(
  h: &H,
  mode: AllocMode,
  tbl: &TableIdent,
) -> Store<H::Io> {
  let mut main = h.open(mode);
  for index in 0..8 {
    main
      .put(tbl, &[up(&key("a", index), &value("a", index), 1)])
      .await
      .unwrap();
  }
  main.create_branch(tbl, "f1").await.unwrap();

  let mut f1 = h.restart(mode, main);
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen(tbl).await.unwrap();
  for index in 0..8 {
    f1.put(tbl, &[up(&key("b", index), &value("b", index), 2)])
      .await
      .unwrap();
  }
  f1.create_archive(tbl, "keep", 0).await.unwrap();
  f1.page_io()
    .remove_manifest_file(tbl, "manifest_f1_0")
    .await
    .unwrap();
  f1
}

/// 孤儿归档冷重开：main 冷进程洪泛（sem-28 下界）+ f1 冷进程 `reopen_archive`。
///
/// `flood=false` 用于 term bump 变体：验证冷态重建的段水位在 term 0→3 下同样
/// 可用（bump 后的水位仍在自己段块内，后续大 value 不 `OutOfSpace`）。
async fn run_prod_orphan_cold<H: ProdBackend>(
  h: &H,
  mode: AllocMode,
  term: u64,
  flood: bool,
  tag: &str,
) {
  let tbl = TableIdent::new(tag, 1).unwrap();
  let f1 = build_prod_orphan(h, mode, &tbl).await;

  let mut cold = h.restart(mode, f1);
  if cold.segment_mapper(&tbl).is_some() || cold.mapper(&tbl).is_some() {
    panic!("{}: 冷 Store 不应已有表状态", h.label());
  }
  if flood {
    cold.prepare_attach("main", 0).unwrap();
    cold.reopen(&tbl).await.unwrap();
    flood_main(&mut cold, &tbl).await;
  }

  // 冷进程重新 attach 到 f1 再重开归档（shard `ReopenArchive` 不先
  // `open_branch`）：内存无表状态、branch manifest 已删。
  let mut arch = h.restart(mode, cold);
  arch.prepare_attach("f1", term).unwrap();
  arch
    .reopen_archive(&tbl, "keep")
    .await
    .unwrap_or_else(|err| {
      panic!(
        "{} mode={mode:?} term={term}: 生产配置下孤儿归档冷重开失败: {err:?}",
        h.label()
      )
    });
  // 纯数据归档只带段块基址预留：重建水位必须落在本分支段块内，后续大 value
  // 仍可写（clamp 到块末尾会直接 OutOfSpace）。
  let watermark = seg_max(&arch, &tbl);
  assert!(
    (BLOCK_FP..BLOCK_FP * 2).contains(&watermark),
    "{} mode={mode:?} term={term}: 重建段水位越界 {watermark}",
    h.label()
  );
  let big = vec![b'L'; BIG_LEN];
  arch
    .put_large(&tbl, &[up(b"after-reopen", &big, 9)])
    .await
    .unwrap_or_else(|err| {
      panic!(
        "{} mode={mode:?} term={term}: 孤儿归档冷重开后大 value 不可写: {err:?}",
        h.label()
      )
    });
  assert_eq!(
    read_large(&arch, &tbl, b"after-reopen").await,
    Some(big),
    "{} mode={mode:?} term={term}: 重建水位后大 value 读回不一致",
    h.label()
  );
  for index in 0..8 {
    assert_eq!(
      arch.get(&tbl, &key("a", index)).await.unwrap(),
      Some(value("a", index)),
      "{} mode={mode:?} term={term}: 归档继承页 a{index} 丢失",
      h.label()
    );
    assert_eq!(
      arch.get(&tbl, &key("b", index)).await.unwrap(),
      Some(value("b", index)),
      "{} mode={mode:?} term={term}: 归档自身页 b{index} 丢失",
      h.label()
    );
  }
}

/// main → f1 → 归档 a1 → big1 → 归档 a2 → 删 branch manifest → 冷态重开 a1：
/// 重建水位必须不低于 a2 的段水位，后续 big2 不得覆盖 a2 仍引用的段页。
async fn run_prod_orphan_later_large<H: ProdBackend>(h: &H, mode: AllocMode, tag: &str) {
  let tbl = TableIdent::new(tag, 1).unwrap();
  let mut main = h.open(mode);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();

  let mut f1 = h.restart(mode, main);
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen(&tbl).await.unwrap();
  f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
  f1.create_archive(&tbl, "a1", 0).await.unwrap();
  let big1 = vec![b'1'; BIG_LEN];
  f1.put_large(&tbl, &[up(b"big1", &big1, 3)]).await.unwrap();
  f1.create_archive(&tbl, "a2", 0).await.unwrap();
  let a2_seg = seg_max(&f1, &tbl);
  assert!(a2_seg > BLOCK_FP, "a2 段水位应高于块基址 {a2_seg}");
  f1.page_io()
    .remove_manifest_file(&tbl, "manifest_f1_0")
    .await
    .unwrap();

  let mut cold = h.restart(mode, f1);
  cold.prepare_attach("f1", 0).unwrap();
  cold.reopen_archive(&tbl, "a1").await.unwrap();
  assert!(
    seg_max(&cold, &tbl) >= a2_seg,
    "{} mode={mode:?}: 冷态重建水位低于更晚归档 a2 的段水位 {} < {a2_seg}",
    h.label(),
    seg_max(&cold, &tbl)
  );
  let big2 = vec![b'2'; BIG_LEN];
  cold
    .put_large(&tbl, &[up(b"big2", &big2, 4)])
    .await
    .unwrap();
  cold.reopen_archive(&tbl, "a2").await.unwrap();
  assert_eq!(
    read_large(&cold, &tbl, b"big1").await,
    Some(big1),
    "{} mode={mode:?}: 冷态重开 a1 后续写的 big2 覆盖了 a2 的段页",
    h.label()
  );
}

/// main → f1 → 归档 a1 → big1 → 建子分支 f2（继承 f1 的 big1 引用）→ 删 f1 的
/// branch manifest → 冷态重开 a1 → big2 → 以 f2 身份读 big1 必须完好。
///
/// f2 的 manifest 里带着继承自 f1 的 data / segment 物理页 id：它们是 f1 段块
/// 内仍被引用的页，冷态重建不能只扫本分支的 manifest（`a1` 早于 big1，段水位
/// 仍是块基址），否则 big2 会覆盖 f2 仍引用的段页。
async fn run_prod_orphan_child_inherit<H: ProdBackend>(h: &H, mode: AllocMode, tag: &str) {
  let tbl = TableIdent::new(tag, 1).unwrap();
  let mut main = h.open(mode);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  main.create_branch(&tbl, "f1").await.unwrap();

  let mut f1 = h.restart(mode, main);
  f1.prepare_attach("f1", 0).unwrap();
  f1.reopen(&tbl).await.unwrap();
  f1.put(&tbl, &[up(b"pre", b"PRE", 2)]).await.unwrap();
  f1.create_archive(&tbl, "a1", 0).await.unwrap();
  let big1 = vec![b'1'; BIG_LEN];
  f1.put_large(&tbl, &[up(b"big1", &big1, 3)]).await.unwrap();
  f1.create_branch(&tbl, "f2").await.unwrap();
  let f1_max_after_big1 = f1.mapper(&tbl).unwrap().max_fp_id();
  f1.page_io()
    .remove_manifest_file(&tbl, "manifest_f1_0")
    .await
    .unwrap();

  let mut cold = h.restart(mode, f1);
  cold.prepare_attach("f1", 0).unwrap();
  cold.reopen_archive(&tbl, "a1").await.unwrap();
  assert!(
    cold.mapper(&tbl).unwrap().max_fp_id() >= f1_max_after_big1,
    "{} mode={mode:?}: 冷态 data 水位低于子分支继承的引用 {} < {f1_max_after_big1}",
    h.label(),
    cold.mapper(&tbl).unwrap().max_fp_id()
  );
  assert!(
    seg_max(&cold, &tbl) > BLOCK_FP,
    "{} mode={mode:?}: 冷态段水位没有覆盖子分支继承的段页",
    h.label()
  );
  let big2 = vec![b'2'; BIG_LEN];
  cold
    .put_large(&tbl, &[up(b"big2", &big2, 4)])
    .await
    .unwrap();

  let mut f2 = h.restart(mode, cold);
  f2.prepare_attach("f2", 0).unwrap();
  f2.reopen(&tbl).await.unwrap();
  assert_eq!(
    read_large(&f2, &tbl, b"big1").await,
    Some(big1),
    "{} mode={mode:?}: 冷态重开 f1 的 a1 后，big2 覆盖了子分支 f2 继承的段页",
    h.label()
  );
}

#[test]
fn prod_orphan_data_archive_cold_reopen_mem() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_prod_orphan_cold(&MemProd, mode, 0, true, &format!("p13_cold_{mode:?}")).await;
    }
  });
}

#[test]
fn prod_orphan_data_archive_cold_reopen_term_bump_mem() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_prod_orphan_cold(&MemProd, mode, 3, false, &format!("p13_term_{mode:?}")).await;
    }
  });
}

#[test]
fn prod_orphan_archive_keeps_later_archive_large_mem() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_prod_orphan_later_large(&MemProd, mode, &format!("p13_later_{mode:?}")).await;
    }
  });
}

#[test]
fn prod_orphan_archive_child_inherited_large_mem() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_prod_orphan_child_inherit(&MemProd, mode, &format!("p13_child_{mode:?}")).await;
    }
  });
}

#[test]
fn prod_orphan_data_archive_cold_reopen_sim() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_prod_orphan_cold(&SimProd, mode, 0, true, &format!("p13_cold_sim_{mode:?}")).await;
    }
  });
}

#[test]
fn prod_orphan_archive_keeps_later_archive_large_sim() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_prod_orphan_later_large(&SimProd, mode, &format!("p13_later_sim_{mode:?}")).await;
    }
  });
}

#[compio::test]
async fn prod_orphan_data_archive_cold_reopen_compio() {
  let root = TempRoot::new("p13_cold_compio");
  let backend = CompioProd { root: &root };
  for mode in PROD_MODES {
    run_prod_orphan_cold(
      &backend,
      mode,
      0,
      true,
      &format!("p13_cold_compio_{mode:?}"),
    )
    .await;
  }
}

#[compio::test]
async fn prod_orphan_data_archive_cold_reopen_term_bump_compio() {
  let root = TempRoot::new("p13_term_compio");
  let backend = CompioProd { root: &root };
  for mode in PROD_MODES {
    run_prod_orphan_cold(
      &backend,
      mode,
      3,
      false,
      &format!("p13_term_compio_{mode:?}"),
    )
    .await;
  }
}

#[compio::test]
async fn prod_orphan_archive_keeps_later_archive_large_compio() {
  let root = TempRoot::new("p13_later_compio");
  let backend = CompioProd { root: &root };
  for mode in PROD_MODES {
    run_prod_orphan_later_large(&backend, mode, &format!("p13_later_compio_{mode:?}")).await;
  }
}

/// 对照组：同一个生产配置归档，在 branch manifest 仍在盘上 / 热态时冷重开
/// 成功。失败只由「branch manifest 缺失 + 冷态」这一组合触发。
#[test]
fn prod_orphan_archive_control_manifest_present_and_warm() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      // 控制组 1：branch manifest 仍在盘上，冷进程重开。
      let tbl = TableIdent::new(format!("p13_ctrl_manifest_{mode:?}"), 1).unwrap();
      let mut main = MemProd.open(mode);
      for index in 0..8 {
        main
          .put(&tbl, &[up(&key("a", index), &value("a", index), 1)])
          .await
          .unwrap();
      }
      main.create_branch(&tbl, "f1").await.unwrap();
      let mut f1 = MemProd.restart(mode, main);
      f1.prepare_attach("f1", 0).unwrap();
      f1.reopen(&tbl).await.unwrap();
      for index in 0..8 {
        f1.put(&tbl, &[up(&key("b", index), &value("b", index), 2)])
          .await
          .unwrap();
      }
      f1.create_archive(&tbl, "keep", 0).await.unwrap();

      let mut cold = MemProd.restart(mode, f1);
      cold.prepare_attach("f1", 0).unwrap();
      cold.reopen_archive(&tbl, "keep").await.unwrap();
      for index in 0..8 {
        assert_eq!(
          cold.get(&tbl, &key("b", index)).await.unwrap(),
          Some(value("b", index))
        );
      }

      // 控制组 2：branch manifest 已删但 Store 仍是热态（内存 segment mapper
      // 提供真实水位）。
      let tbl = TableIdent::new(format!("p13_ctrl_warm_{mode:?}"), 1).unwrap();
      let mut f1 = build_prod_orphan(&MemProd, mode, &tbl).await;
      f1.reopen_archive(&tbl, "keep").await.unwrap();
      for index in 0..8 {
        assert_eq!(
          f1.get(&tbl, &key("a", index)).await.unwrap(),
          Some(value("a", index))
        );
      }
    }
  });
}

/// 对照组：纯数据 Store（未配置 large value，仓库 sem-28 用例的配置）的孤儿
/// 归档冷重开不受影响。
#[test]
fn plain_orphan_archive_cold_reopen_mem() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      let tbl = TableIdent::new(format!("p13_plain_{mode:?}"), 1).unwrap();
      let mut main = mem_store(mode);
      build_orphan(&mut main, &tbl).await;
      let mut f1 = reopen(mode, main.into_io());
      f1.open_branch(&tbl, "f1", 0).await.unwrap();
      for index in 0..8 {
        f1.put(&tbl, &[up(&key("b", index), &value("b", index), 2)])
          .await
          .unwrap();
      }
      f1.create_archive(&tbl, "keep", 0).await.unwrap();
      f1.page_io()
        .remove_manifest_file(&tbl, "manifest_f1_0")
        .await
        .unwrap();

      let mut cold = reopen(mode, f1.into_io());
      cold.prepare_attach("f1", 0).unwrap();
      cold
        .reopen_archive(&tbl, "keep")
        .await
        .unwrap_or_else(|err| panic!("mode={mode:?}: 纯数据归档冷重开失败: {err:?}"));
      for index in 0..8 {
        assert_eq!(
          cold.get(&tbl, &key("a", index)).await.unwrap(),
          Some(value("a", index))
        );
        assert_eq!(
          cold.get(&tbl, &key("b", index)).await.unwrap(),
          Some(value("b", index))
        );
      }
    }
  });
}

// sem-36：C++ / legacy 未对齐区间链下的冷态证据窗口
//
// Rust `create_branch` 把新分支的分配起点对齐到 1024 文件块，归档水位恒
// `>= 派生块基址`，冷态证据窗口取 `[块基址, 块末尾)` 是安全的。C++ / legacy
// `SetBranchFileIdTerm` 追加的区间链未对齐：branch 的 data / segment 文件从
// `父区间上界 + 1` 开始，归档水位可能远低于 Rust 派生的块基址。子分支继承的
// 低地址页落在 `[归档水位, 块基址)`，只扫块内会漏掉它们，续写覆盖存活子分支
// still 引用的页（无错误码）。这里手工构造该布局（不依赖 C++ 二进制）：
//
// 1. main 写若干 key；手写 f1 的 branch manifest，区间链 [main=5, f1=55]，
//    `max_fp_id = 24`（页 24 = 文件 6，C++ 未对齐），f1 从低地址开始写；
// 2. f1 写 300 条 200B；手写归档 a1（早于这 300 条，水位 24 / 对照组 1000）
//    与 f2（区间链 [main=5, f1=55, f2=56]，快照取 f1 真实 root / mapping）；
// 3. 删除 `manifest_f1_0`（sem-28 孤儿形态），冷 Store `reopen_archive(a1)`；
// 4. 续写 400 条后以 f2 身份读回 300 条：任一页被覆盖即失败。

const CPP_KEYS: u32 = 300;
const CPP_VALUE_LEN: usize = 200;
/// C++ 区间链形状：父区间上界 / 本分支上界均未按 1024 文件对齐。
const CPP_MAIN_FILES: FileId = 5;
const CPP_F1_FILES: FileId = 55;
/// f1 的首个 data 页 = `(父上界 + 1) << shift` = 页 24（文件 6）。
const CPP_F1_START: FilePageId = 24;
/// 归档水位：低于 f2 继承页（C++ 布局），也低于派生块基址 4096。
const CPP_ARCHIVE_MAX: FilePageId = CPP_F1_START;
/// 对照组：归档水位高于 f2 的全部页。
const CPP_ARCHIVE_MAX_HIGH: FilePageId = 1000;
/// C++ segment 区间：f1 的段文件从 file 1 起（页 4），同样低于派生段块基址。
const CPP_SEG_BASE: FilePageId = 4;
/// 7 段：从 0 起写会覆盖继承段页 4..6。
const CPP_SEG_LEN: usize = 28_000;

fn cpp_value(prefix: &str, index: u32) -> Vec<u8> {
  let tag = format!("{prefix}{index}");
  let mut value = vec![b'.'; CPP_VALUE_LEN.max(tag.len())];
  value[..tag.len()].copy_from_slice(tag.as_bytes());
  value
}

/// C++ `SetBranchFileIdTerm` 的区间：父 / 本分支上界是实际写到的文件 id。
fn cpp_range(branch: &str, max_file_id: FileId, max_segment_file_id: FileId) -> BranchFileRange {
  BranchFileRange {
    branch_name: branch.into(),
    term: 0,
    max_file_id,
    max_segment_file_id,
  }
}

fn cpp_metadata(branch: &str, chain: &[BranchFileRange]) -> BranchManifestMetadata {
  BranchManifestMetadata {
    branch_name: branch.into(),
    term: 0,
    file_range_li: chain.to_vec(),
  }
}

/// 空快照 manifest（root / mapping 为空）：用于手写 branch / archive。
///
/// `max_segment_fp_id > 0` 时写段段（空 mapping + 预留段基址），复刻 Rust
/// `create_branch` 在 `use_segment` 下写入的形态。
fn cpp_empty_manifest(
  branch: &str,
  chain: &[BranchFileRange],
  max_fp_id: FilePageId,
  max_segment_fp_id: FilePageId,
) -> Vec<u8> {
  let metadata = cpp_metadata(branch, chain);
  let segment = (max_segment_fp_id > 0).then(empty::<u64>);
  ManifestBuilder::new()
    .snapshot_with(
      SnapshotArg {
        root_id: NULL_PAGE,
        ttl_root: NULL_PAGE,
        max_fp_id,
        dict_bytes: &[],
        branch_metadata: &metadata,
        max_segment_fp_id,
      },
      empty::<u64>(),
      segment,
    )
    .unwrap()
}

/// 带真实快照内容的 manifest：root / mapping / segment mapping 取 replay 结果。
fn cpp_snapshot_manifest(
  branch: &str,
  chain: &[BranchFileRange],
  state: &fkv_recovery::ReplayState,
) -> Vec<u8> {
  let metadata = cpp_metadata(branch, chain);
  let segment = (!state.segment_mapping.is_empty()).then(|| state.segment_mapping.iter_value_li());
  ManifestBuilder::new()
    .snapshot_with(
      SnapshotArg {
        root_id: state.root_id,
        ttl_root: state.ttl_root_id,
        max_fp_id: state.max_fp_id,
        dict_bytes: &state.dict_bytes,
        branch_metadata: &metadata,
        max_segment_fp_id: state.max_segment_fp_id,
      },
      state.mapping_tbl.iter_value_li(),
      segment,
    )
    .unwrap()
}

/// 读具名 manifest 并 replay 成状态。
async fn replay_manifest<I: PageIo + ManifestIo>(
  io: &I,
  tbl: &TableIdent,
  name: &str,
) -> fkv_recovery::ReplayState {
  let bytes = io
    .read_manifest_file(tbl, name)
    .await
    .unwrap()
    .unwrap_or_else(|| panic!("manifest 缺失: {name}"));
  fkv_task_reopen::replay_manifest_bytes_with_len(&bytes)
    .unwrap()
    .0
}

/// 写 `count` 条 200B 的 `prefixN` key，返回落盘后的最大映射页。
async fn put_cpp_keys<I: PageIo + ManifestIo>(
  store: &mut Store<I>,
  tbl: &TableIdent,
  prefix: &str,
  count: u32,
  ts_base: u64,
) {
  let mut written = 0;
  while written < count {
    let end = (written + 40).min(count);
    let entries: Vec<_> = (written..end)
      .map(|n| {
        BatchEntry::upsert(
          format!("{prefix}{n}").into_bytes(),
          cpp_value(prefix, n),
          ts_base + u64::from(n),
        )
      })
      .collect();
    store.put(tbl, &entries).await.unwrap();
    written = end;
  }
}

fn max_mapped_page(state: &fkv_recovery::ReplayState) -> FilePageId {
  state
    .mapping_tbl
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(decode_id)
    .max()
    .unwrap_or(0)
}

/// C++ 布局 + 低地址归档：冷重开归档后续写不得覆盖 f2 继承的 data 页。
async fn run_cpp_low_range_data<H: ProdBackend>(
  h: &H,
  mode: AllocMode,
  archive_max: FilePageId,
  tag: &str,
) {
  let tbl = TableIdent::new(tag, 1).unwrap();
  // 1. main 建立真实快照；f1 的区间链为 C++ 未对齐形状 [main=5, f1=55]。
  let mut main = h.open(mode);
  for index in 0..8 {
    main
      .put(&tbl, &[up(&key("a", index), &value("a", index), 1)])
      .await
      .unwrap();
  }
  let chain_f1 = [
    cpp_range("main", CPP_MAIN_FILES, 0),
    cpp_range("f1", CPP_F1_FILES, 0),
  ];
  main
    .page_io()
    .write_manifest_file(
      &tbl,
      "manifest_f1_0",
      &cpp_empty_manifest("f1", &chain_f1, CPP_F1_START, 0),
    )
    .await
    .unwrap();

  // 2. f1 从页 24（文件 6）开始写 300 条，得到真实的 root / mapping / 水位。
  let mut f1 = h.restart(mode, main);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  put_cpp_keys(&mut f1, &tbl, "base", CPP_KEYS, 2).await;
  let state = replay_manifest(f1.page_io(), &tbl, "manifest_f1_0").await;
  let base_max = state.max_fp_id;
  let max_page = max_mapped_page(&state);
  assert!(
    base_max > CPP_F1_START && max_page < BLOCK_FP,
    "{} mode={mode:?}: 构造的 f1 必须落在低地址（base_max={base_max} max_page={max_page}）",
    h.label()
  );

  // 3. 手写归档 a1（早于 300 条）与继承真实快照的 f2。
  f1.page_io()
    .write_manifest_file(
      &tbl,
      "manifest_f1_0_a1",
      &cpp_empty_manifest("f1", &chain_f1, archive_max, 0),
    )
    .await
    .unwrap();
  let chain_f2 = [
    cpp_range("main", CPP_MAIN_FILES, 0),
    cpp_range("f1", CPP_F1_FILES, 0),
    cpp_range("f2", CPP_F1_FILES + 1, 1),
  ];
  f1.page_io()
    .write_manifest_file(
      &tbl,
      "manifest_f2_0",
      &cpp_snapshot_manifest("f2", &chain_f2, &state),
    )
    .await
    .unwrap();
  // sem-28 的孤儿形态：branch manifest 已删，只留归档与存活子分支。
  f1.page_io()
    .remove_manifest_file(&tbl, "manifest_f1_0")
    .await
    .unwrap();

  // 4. 冷 Store 重开 a1：重建水位必须越过 f2 继承的低地址页。
  let mut cold = h.restart(mode, f1);
  cold.prepare_attach("f1", 0).unwrap();
  cold.reopen_archive(&tbl, "a1").await.unwrap();
  let cold_max = cold.mapper(&tbl).unwrap().max_fp_id();
  assert!(
    cold_max > max_page,
    "{} mode={mode:?} archive_max={archive_max}: 冷态 data 水位 {cold_max} \
     未覆盖 f2 继承页（最大页 {max_page}）",
    h.label()
  );

  // 5. 续写足够多的 data（400 条 ≈ 256 页），再以 f2 身份读回全部 key。
  put_cpp_keys(&mut cold, &tbl, "new", 400, 1000).await;
  let mut f2 = h.restart(mode, cold);
  f2.prepare_attach("f2", 0).unwrap();
  f2.reopen(&tbl).await.unwrap();
  for index in 0..CPP_KEYS {
    assert_eq!(
      f2.get(&tbl, &key("base", index)).await.unwrap(),
      Some(cpp_value("base", index)),
      "{} mode={mode:?} archive_max={archive_max}: 冷态续写覆盖了 f2 继承页 base{index}",
      h.label()
    );
  }
}

/// C++ 布局 + 归档没有段状态（`max_segment_fp_id = 0`）：f1 在归档之后写入的
/// 大 value 段页（页 4..）被 f2 继承，冷重开 a1 的段水位不能停在 0。
async fn run_cpp_low_range_segment<H: ProdBackend>(
  h: &H,
  mode: AllocMode,
  archive_seg_max: FilePageId,
  tag: &str,
) {
  let tbl = TableIdent::new(tag, 1).unwrap();
  let mut main = h.open(mode);
  main.put(&tbl, &[up(b"a", b"A", 1)]).await.unwrap();
  let chain_f1 = [
    cpp_range("main", CPP_MAIN_FILES, 0),
    cpp_range("f1", CPP_F1_FILES, 1),
  ];
  // f1 的段区间从页 4 起（C++ 未对齐）；先只预留段基址，尚无段页。
  main
    .page_io()
    .write_manifest_file(
      &tbl,
      "manifest_f1_0",
      &cpp_empty_manifest("f1", &chain_f1, CPP_F1_START, CPP_SEG_BASE),
    )
    .await
    .unwrap();

  let mut f1 = h.restart(mode, main);
  f1.open_branch(&tbl, "f1", 0).await.unwrap();
  let big1 = vec![b'1'; CPP_SEG_LEN];
  f1.put_large(&tbl, &[up(b"big1", &big1, 2)]).await.unwrap();
  let allocated = f1.segment_mapper(&tbl).unwrap().max_fp_id();
  assert!(
    allocated > CPP_SEG_BASE && allocated < BLOCK_FP,
    "{} mode={mode:?}: f1 的段页必须落在低地址（allocated={allocated}）",
    h.label()
  );
  let state = replay_manifest(f1.page_io(), &tbl, "manifest_f1_0").await;

  // a1 早于 big1：C++ 归档没有段段（`max_segment_fp_id = 0`）；对照组带 >0
  // 的预留段水位，旧代码即可抬到块基址。
  f1.page_io()
    .write_manifest_file(
      &tbl,
      "manifest_f1_0_a1",
      &cpp_empty_manifest("f1", &chain_f1, CPP_F1_START, archive_seg_max),
    )
    .await
    .unwrap();
  let chain_f2 = [
    cpp_range("main", CPP_MAIN_FILES, 0),
    cpp_range("f1", CPP_F1_FILES, 1),
    cpp_range("f2", CPP_F1_FILES + 1, 2),
  ];
  f1.page_io()
    .write_manifest_file(
      &tbl,
      "manifest_f2_0",
      &cpp_snapshot_manifest("f2", &chain_f2, &state),
    )
    .await
    .unwrap();
  f1.page_io()
    .remove_manifest_file(&tbl, "manifest_f1_0")
    .await
    .unwrap();

  let mut cold = h.restart(mode, f1);
  cold.prepare_attach("f1", 0).unwrap();
  cold.reopen_archive(&tbl, "a1").await.unwrap();
  let cold_seg = cold
    .segment_mapper(&tbl)
    .map_or(0, |mapper| mapper.max_fp_id());
  assert!(
    cold_seg >= BLOCK_FP,
    "{} mode={mode:?} archive_seg_max={archive_seg_max}: 冷态段水位 {cold_seg} \
     未抬到块基址，续写会覆盖 f2 继承的段页",
    h.label()
  );

  let big2 = vec![b'2'; CPP_SEG_LEN];
  cold
    .put_large(&tbl, &[up(b"big2", &big2, 3)])
    .await
    .unwrap();
  let mut f2 = h.restart(mode, cold);
  f2.prepare_attach("f2", 0).unwrap();
  f2.reopen(&tbl).await.unwrap();
  assert_eq!(
    read_large(&f2, &tbl, b"big1").await,
    Some(big1),
    "{} mode={mode:?} archive_seg_max={archive_seg_max}: 冷态续写覆盖了 f2 继承的段页",
    h.label()
  );
}

#[test]
fn cpp_layout_low_range_data_mem() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_cpp_low_range_data(
        &MemProd,
        mode,
        CPP_ARCHIVE_MAX,
        &format!("s36_cpp_bug_{mode:?}"),
      )
      .await;
      run_cpp_low_range_data(
        &MemProd,
        mode,
        CPP_ARCHIVE_MAX_HIGH,
        &format!("s36_cpp_ctrl_{mode:?}"),
      )
      .await;
    }
  });
}

#[compio::test]
async fn cpp_layout_low_range_data_compio() {
  let root = TempRoot::new("s36_cpp_compio");
  let backend = CompioProd { root: &root };
  for mode in PROD_MODES {
    run_cpp_low_range_data(
      &backend,
      mode,
      CPP_ARCHIVE_MAX,
      &format!("s36_cpp_compio_bug_{mode:?}"),
    )
    .await;
    run_cpp_low_range_data(
      &backend,
      mode,
      CPP_ARCHIVE_MAX_HIGH,
      &format!("s36_cpp_compio_ctrl_{mode:?}"),
    )
    .await;
  }
}

#[test]
fn cpp_layout_low_range_segment_mem() {
  pollster::block_on(async {
    for mode in PROD_MODES {
      run_cpp_low_range_segment(&MemProd, mode, 0, &format!("s36_seg_bug_{mode:?}")).await;
      run_cpp_low_range_segment(
        &MemProd,
        mode,
        CPP_ARCHIVE_MAX_HIGH,
        &format!("s36_seg_ctrl_{mode:?}"),
      )
      .await;
    }
  });
}
