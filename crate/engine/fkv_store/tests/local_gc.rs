//! Local 模式下的 `LocalGc`：没有远端副本，只回收本地无引用文件，判定集合与
//! `FileGc` 一致。

use fkv_btree::BatchEntry;
use fkv_io::FileIo;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

const SHIFT: u8 = 1;

#[test]
fn local_gc_reclaims_unreferenced_local_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store = Store::new(MemStore::with_pages_per_file(128, SHIFT), 128, 8)
      .with_pages_per_file_shift(SHIFT);
    let batch: Vec<BatchEntry> = (0..200u32)
      .map(|index| {
        BatchEntry::upsert(
          format!("k{index:03}").into_bytes(),
          vec![b'v'; 40],
          u64::from(index) + 1,
        )
      })
      .collect();
    store.put(&tbl, &batch).await.unwrap();
    let before = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(before.len() > 1, "应写出多个 data 文件");

    // 清空内容：旧文件失去引用，但 manifest 仍可能保护当前尾文件。
    store.truncate(&tbl).await.unwrap();
    let removed = store.local_gc(&tbl, SHIFT).await.unwrap();
    assert!(removed >= 1, "LocalGc 应回收无引用的本地文件");
    let after = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(
      after.len() < before.len(),
      "LocalGc 后文件数应下降：before={before:?} after={after:?}"
    );
    // 表仍可继续写读（manifest / 内存状态未被破坏）。
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 9)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"k").await.unwrap().unwrap(), b"v");
  });
}
