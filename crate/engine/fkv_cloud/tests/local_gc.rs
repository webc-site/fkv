//! `LocalGc` / `FileGc` 的删除范围差异：LocalGc 只清本地 cache，远端对象原样
//! 保留；FileGc 才删除远端副本（对齐 C++ `RunLocalFileGc` / `RunFileGc`）。

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io::FileIo;
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::Store;
use fkv_types::TableIdent;

const SHIFT: u8 = 1;
const PAGE_SIZE: usize = 128;

fn cloud(remote: &MemObjectStore) -> CloudStore<MemStore, MemObjectStore> {
  CloudStore::new(
    MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
    remote.clone(),
    "fkv",
    SHIFT,
  )
}

fn store(remote: &MemObjectStore) -> Store<CloudStore<MemStore, MemObjectStore>> {
  Store::new(cloud(remote), PAGE_SIZE, 8).with_pages_per_file_shift(SHIFT)
}

/// 远端 data 对象 key（不含 manifest / segment）。
async fn remote_data_key_li(remote: &MemObjectStore, tbl: &TableIdent) -> Vec<String> {
  remote
    .list(&format!("fkv/{tbl}/"))
    .await
    .unwrap()
    .into_iter()
    .map(|meta| meta.key)
    .filter(|key| {
      key
        .rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("data_"))
    })
    .collect()
}

async fn write_then_truncate(
  store: &mut Store<CloudStore<MemStore, MemObjectStore>>,
  tbl: &TableIdent,
) {
  let batch: Vec<BatchEntry> = (0..200u32)
    .map(|index| {
      BatchEntry::upsert(
        format!("k{index:03}").into_bytes(),
        vec![b'v'; 40],
        u64::from(index) + 1,
      )
    })
    .collect();
  store.put(tbl, &batch).await.unwrap();
  // 清空内容并落一份 root 为空的 manifest：旧数据文件全部失去引用。
  store.truncate(tbl).await.unwrap();
}

#[test]
fn local_gc_deletes_local_cache_only_and_keeps_remote_objects() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = store(&remote);
    write_then_truncate(&mut store, &tbl).await;

    let remote_before = remote_data_key_li(&remote, &tbl).await;
    let local_before = store.page_io().list_local_file_id_li(&tbl).await.unwrap();
    assert!(!remote_before.is_empty(), "写入后应有远端 data 对象");
    assert!(!local_before.is_empty(), "写入后应有本地 cache 文件");

    // LocalGc：删本地无引用文件，远端对象一个都不能少。
    let removed = store.local_gc(&tbl, SHIFT).await.unwrap();
    assert!(removed >= 1, "LocalGc 应回收无引用的本地文件");
    assert_eq!(
      remote_data_key_li(&remote, &tbl).await,
      remote_before,
      "LocalGc 不得删除远端对象"
    );
    let local_after = store.page_io().list_local_file_id_li(&tbl).await.unwrap();
    assert!(
      local_after.len() < local_before.len(),
      "LocalGc 应缩小本地 cache：before={local_before:?} after={local_after:?}"
    );
    // 本地 cache 缺失的文件仍可由云端 FileIo 从远端枚举出来。
    let union_li = store.page_io().list_file_id_li(&tbl).await.unwrap();
    assert!(
      union_li.len() >= local_after.len(),
      "云端列举应合并本地 cache 与远端对象"
    );

    // FileGc：按模式删远端副本。
    let removed = store.file_gc(&tbl, SHIFT).await.unwrap();
    assert!(removed >= 1, "FileGc 应回收无引用的远端对象");
    assert!(
      remote_data_key_li(&remote, &tbl).await.is_empty(),
      "FileGc 后远端 data 对象应清空"
    );
  });
}
