use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo, PageIo, TermIo};
use fkv_io_mem::MemStore;
use fkv_types::TableIdent;

#[test]
fn page_roundtrip() {
  pollster::block_on(async {
    let store = MemStore::new(8);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 7, b"12345678").await.unwrap();
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 7, &mut dst).await.unwrap();
    assert_eq!(&dst, b"12345678");
    let mut missing = [0u8; 8];
    assert_eq!(
      store.read_page(&tbl, 8, &mut missing).await.unwrap_err(),
      KvError::ResourceMissing
    );
  });
}

#[test]
fn manifest_roundtrip() {
  pollster::block_on(async {
    let store = MemStore::new(8);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.append_manifest(&tbl, b"abc").await.unwrap();
    store.append_manifest(&tbl, b"def").await.unwrap();
    assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"abcdef");
    store.switch_manifest(&tbl, b"xyz").await.unwrap();
    assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"xyz");
    store.drop_manifest(&tbl).await.unwrap();
    assert!(store.read_manifest(&tbl).await.unwrap().is_none());
  });
}

#[test]
fn term_roundtrip() {
  pollster::block_on(async {
    let store = MemStore::new(8);
    assert_eq!(store.read_term("main", 1).await.unwrap(), None);
    store.upsert_term("main", 1, 5).await.unwrap();
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(5));
    assert_eq!(
      store.upsert_term("main", 1, 4).await.unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

/// `list_manifest_file_name_li` 只返回命名规则内的 manifest；`segcomplete*`
/// 等旁路名必须留给 `list_sidecar_file_name_li`（GC 会把名单当 manifest replay）。
#[test]
fn list_manifest_file_name_li_excludes_sidecar_names() {
  pollster::block_on(async {
    let store = MemStore::new(8);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store
      .write_manifest_file(&tbl, "manifest_main_0", b"m")
      .await
      .unwrap();
    store
      .write_manifest_file(&tbl, "segcomplete_3", b"magic")
      .await
      .unwrap();
    assert_eq!(
      store.list_manifest_file_name_li(&tbl).await.unwrap(),
      vec!["manifest_main_0".to_owned()]
    );
    assert_eq!(
      store.list_sidecar_file_name_li(&tbl).await.unwrap(),
      vec!["segcomplete_3".to_owned()]
    );
  });
}

/// io-41：`list_*` 的契约是**升序去重**（compio / sim 都排序）。Mem 之前直接吐
/// `HashSet` 的迭代顺序，依赖顺序的 GC 遍历 / 确定性测试在 Mem 后端不可复现。
#[test]
fn list_file_id_li_is_sorted_ascending() {
  pollster::block_on(async {
    let store = MemStore::with_pages_per_file(8, 4);
    let tbl = TableIdent::new("demo", 1).unwrap();
    // 乱序写入 6 个不同 file id（page = file_id << shift），并混入 0 与最大值。
    for file_id in [7u64, 0, 3, 5, 1, 9] {
      store
        .write_page(&tbl, file_id << 4, b"12345678")
        .await
        .unwrap();
    }
    // 同一 file 两个页只算一个 file id。
    store
      .write_page(&tbl, (3u64 << 4) | 2, b"12345678")
      .await
      .unwrap();

    let file_id_li = store.list_file_id_li(&tbl).await.unwrap();
    assert_eq!(file_id_li, vec![0, 1, 3, 5, 7, 9]);
    assert!(file_id_li.is_sorted(), "{file_id_li:?}");
  });
}
