//! GC 的 in-flight 窗口保护：数据文件已写入、增量 manifest 尚未提交时不得删除。

use fkv_btree::BatchEntry;
use fkv_io::FileIo;
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::TableIdent;

/// core-23：本进程 allocator 已推进（文件已写）但 manifest 还没记录的文件，
/// 必须落在 per-branch 高水位之内，GC 不得删除。
#[test]
fn gc_keeps_file_written_before_manifest_commit() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut store =
      Store::new(MemStore::with_pages_per_file(512, 1), 512, 8).with_pages_per_file_shift(1);
    let batch: Vec<BatchEntry> = (0..40u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'v'; 60], 1))
      .collect();
    store.put(&tbl, &batch).await.unwrap();

    let max_file_id = store.mapper(&tbl).unwrap().max_fp_id() >> 1;
    // 模拟写路径：allocator 已分配新文件并落盘，manifest append 尚未发生。
    let inflight = max_file_id + 1;
    store
      .page_io()
      .write_file(&tbl, inflight, &[0x5au8; 512])
      .await
      .unwrap();

    store.file_gc(&tbl, 1).await.unwrap();

    let file_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(
      file_li.contains(&inflight),
      "in-flight 文件被 GC 删除: {file_li:?}"
    );
    assert_eq!(
      store.get(&tbl, b"k000").await.unwrap(),
      Some(vec![b'v'; 60])
    );
  });
}

/// 未打开表的 GC（维护进程 / 别的实例视角）没有 active branch 可播种，
/// 高水位之上的文件可能属于远端写入者尚未提交的 in-flight 写入，必须保留。
#[test]
fn gc_without_open_table_keeps_files_above_manifest_high_water() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut store =
      Store::new(MemStore::with_pages_per_file(512, 1), 512, 8).with_pages_per_file_shift(1);
    let batch: Vec<BatchEntry> = (0..40u32)
      .map(|index| BatchEntry::upsert(format!("k{index:03}").into_bytes(), vec![b'v'; 60], 1))
      .collect();
    store.put(&tbl, &batch).await.unwrap();

    let max_file_id = store.mapper(&tbl).unwrap().max_fp_id() >> 1;
    let inflight = max_file_id + 1;
    store
      .page_io()
      .write_file(&tbl, inflight, &[0x5au8; 512])
      .await
      .unwrap();

    // 新 Store 不打开表：GC 只有 manifest 信息，没有 in-memory 高水位。
    let fresh = Store::new(store.into_io(), 512, 8).with_pages_per_file_shift(1);
    fresh.file_gc(&tbl, 1).await.unwrap();

    let file_li = fresh.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(
      file_li.contains(&inflight),
      "无 active branch 时高水位之上的文件被误删: {file_li:?}"
    );
  });
}
