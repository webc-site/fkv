//! io-02：`sync_data` 的 fdatasync 并发度跟随本轮脏文件数，并受
//! `max_inflight_write` 封顶；不再被硬编码的 16 分波。
//!
//! 用 `sync_inflight_peak()` 直接观察同时在飞的 fdatasync 数：每个同步
//! future 进入时计数 +1、完成（或取消）时 -1，峰值由 `CompioStore` 记录。

use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
/// 每个 data 文件 1 页：`file_page_id = file_id << shift`。
const PAGES_PER_FILE_SHIFT: u8 = 2;
/// 构造 64 个脏文件（恰为旧硬编码上限 16 的 4 倍）。
const FILE_NUM: usize = 64;

fn store_at(name: &str) -> (TempRoot, CompioStore) {
  let root = TempRoot::new(name);
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, PAGES_PER_FILE_SHIFT)
    .with_checksum_verify(false);
  (root, store)
}

/// 让 `FILE_NUM` 个不同 data 文件各变脏一页。
async fn mark_dirty_files(store: &CompioStore, tbl: &TableIdent) {
  let page = vec![0x5A; PAGE_SIZE];
  for file_id in 0..FILE_NUM as u64 {
    store
      .write_page(tbl, file_id << PAGES_PER_FILE_SHIFT, &page)
      .await
      .unwrap();
  }
  assert_eq!(store.dirty_file_num(tbl), FILE_NUM, "夹具前提：64 个脏文件");
}

/// 默认 `max_inflight_write`（32 << 10）远大于脏文件数：一轮 64 个文件
/// 同时提交，而不是按 16 分四波。旧实现下峰值是 16，本断言会失败。
#[compio::test]
async fn sync_data_launches_all_dirty_files_in_one_wave() {
  let (_root, store) = store_at("sync_inflight_default");
  let tbl = TableIdent::new("demo", 1).unwrap();
  mark_dirty_files(&store, &tbl).await;

  store.sync_data(&tbl).await.unwrap();

  assert_eq!(store.dirty_file_num(&tbl), 0);
  assert_eq!(
    store.sync_inflight_peak(),
    FILE_NUM,
    "默认配置下一轮脏文件应全部同时在飞"
  );
}

/// 配置收紧到 4 时按配置封顶：峰值等于 `min(64, 4)`，证明并发上限来自
/// `max_inflight_write`，不是另一个魔法数字。
#[compio::test]
async fn sync_data_inflight_peak_is_capped_by_config() {
  let (_root, store) = store_at("sync_inflight_capped");
  let store = store.with_max_inflight_write(4);
  let tbl = TableIdent::new("demo", 2).unwrap();
  mark_dirty_files(&store, &tbl).await;

  store.sync_data(&tbl).await.unwrap();

  assert_eq!(store.dirty_file_num(&tbl), 0);
  assert_eq!(
    store.sync_inflight_peak(),
    4,
    "并发度应按 max_inflight_write 封顶"
  );
}
