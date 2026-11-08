//! sem-58：truncate 路径的 manifest 冲刷形态。
//!
//! Form A（对齐 C++）：已加载且两棵树均空的分区上，整表 truncate 是纯 no-op
//! —— 零释放、零 manifest 写、永不创建 manifest 对象，对齐 C++
//! `BatchWriteTask::Truncate` 在 trunc_pos 分派之前的
//! `root_id_ == MaxPageId` 无条件早退（batch_write_task.cpp:1849-1852）。
//! 覆盖三个子形态：懒开空表（A1）、已清空分区（A2）、维护收尾后的分区（A3）；
//! 反向对照锁定非空分区 truncate 仍整写快照。
//!
//! Form B（登记为有意差异，COMPAT_BASELINE l2-03 sem-58 行）：无匹配的
//! `truncate_from` 在 fkv 是真 no-op；C++ 同输入会 COW 重写最右路径并
//! append 一条 record。本文件锁定 fkv 侧零写入口径。

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_io::{FileIo, ManifestIo};
use fkv_io_mem::MemStore;
use fkv_store::Store;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, NULL_PAGE, TableIdent};

#[test]
fn truncate_on_lazy_opened_empty_partition_creates_no_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem58_lazy", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    // 懒开空表（SDK ensure_open 同路径）：mapper 就位、root 为空树、无 manifest。
    store.open_empty(&tbl);
    store.truncate(&tbl).await.unwrap();

    assert_eq!(
      store.page_io().manifest_switch_num(),
      0,
      "空分区 truncate 不得创建 manifest（C++ 早退）"
    );
    assert_eq!(store.page_io().manifest_append_num(), 0);
    assert_eq!(
      store.page_io().read_manifest(&tbl).await.unwrap(),
      None,
      "manifest 文件不得存在"
    );
    assert!(
      store
        .page_io()
        .list_manifest_file_name_li(&tbl)
        .await
        .unwrap()
        .is_empty(),
      "manifest 对象集必须为空"
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 0);
    assert!(
      matches!(store.reopen(&tbl).await, Err(KvError::ResourceMissing)),
      "无 manifest 分区的 reopen 语义不得改变"
    );

    // 早退不吞后续真实写批：首个有效写批照常建 manifest。
    store
      .put(&tbl, &[BatchEntry::upsert(b"k".to_vec(), b"V".to_vec(), 1)])
      .await
      .unwrap();
    assert_eq!(store.page_io().manifest_switch_num(), 1);
    assert_eq!(store.get(&tbl, b"k").await.unwrap(), Some(b"V".to_vec()));
  });
}

#[test]
fn truncate_on_emptied_partition_keeps_manifest_untouched() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem58_emptied", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"k1".to_vec(), b"V".to_vec(), 1)],
      )
      .await
      .unwrap();
    store.delete(&tbl, b"k1", 2).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(NULL_PAGE));
    let switches = store.page_io().manifest_switch_num();
    let appends = store.page_io().manifest_append_num();
    let bytes = store
      .page_io()
      .read_manifest(&tbl)
      .await
      .unwrap()
      .expect("清空后 manifest 仍在（快照 + 删光批 delta）");

    // A2：已清空分区的整表 truncate（含重复调用）零 manifest 写，
    // 既有「快照 + delta」原样保留（C++ 早退）。
    store.truncate(&tbl).await.unwrap();
    store.truncate(&tbl).await.unwrap();
    assert_eq!(store.page_io().manifest_switch_num(), switches);
    assert_eq!(store.page_io().manifest_append_num(), appends);
    assert_eq!(
      store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
      bytes,
      "空分区 truncate 不得改写 manifest"
    );
    assert_eq!(store.root_id(&tbl), Some(NULL_PAGE));
    assert_eq!(store.ttl_root_id(&tbl), Some(NULL_PAGE));
    assert_eq!(store.get(&tbl, b"k1").await.unwrap(), None);

    // 反向对照：非空分区的 truncate 仍整写快照（truncate.rs 现行为不变）。
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"k2".to_vec(), b"W".to_vec(), 3)],
      )
      .await
      .unwrap();
    let (nonempty_switches, nonempty_appends) = (
      store.page_io().manifest_switch_num(),
      store.page_io().manifest_append_num(),
    );
    store.truncate(&tbl).await.unwrap();
    assert_eq!(
      store.page_io().manifest_switch_num(),
      nonempty_switches + 1,
      "非空分区 truncate 必须整写快照"
    );
    assert_eq!(store.page_io().manifest_append_num(), nonempty_appends);
    assert_eq!(store.root_id(&tbl), Some(NULL_PAGE));
    assert_eq!(store.get(&tbl, b"k2").await.unwrap(), None);
  });
}

#[test]
fn truncate_after_partition_cleanup_writes_no_manifest() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem58_cleanup", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    store
      .put(
        &tbl,
        &[BatchEntry::upsert(b"k1".to_vec(), b"V".to_vec(), 1)],
      )
      .await
      .unwrap();
    // 非空分区 truncate：整写空快照；随后 GC 删光 data 文件并做分区收尾。
    store.truncate(&tbl).await.unwrap();
    store
      .file_gc(&tbl, DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    store.maintain(&tbl).await.unwrap();
    assert_eq!(store.root_id(&tbl), Some(NULL_PAGE));
    assert!(
      store
        .page_io()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );

    // 收尾后的状态不锁（cleanup 是否保留 manifest 由 sem-59 口径决定），
    // 只锁其后的 truncate() 零 manifest 写：A3 = 收尾后不得凭空（重）建 manifest。
    let switches = store.page_io().manifest_switch_num();
    let appends = store.page_io().manifest_append_num();
    let bytes = store.page_io().read_manifest(&tbl).await.unwrap();
    store.truncate(&tbl).await.unwrap();
    assert_eq!(store.page_io().manifest_switch_num(), switches);
    assert_eq!(store.page_io().manifest_append_num(), appends);
    assert_eq!(
      store.page_io().read_manifest(&tbl).await.unwrap(),
      bytes,
      "收尾后的空分区 truncate 不得改动 manifest 状态"
    );
  });
}

/// Form B（登记口径）：无匹配的 `truncate_from` 是真 no-op —— 零 manifest 写。
/// C++ 同输入会 COW 重写最右路径并 append 一条 record（该差异不对拍）；
/// 对照组锁定有匹配时仍正常删除并 append。
#[test]
fn no_match_truncate_from_is_a_true_noop() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem58_nomatch", 1).unwrap();
    let mut store = Store::new(MemStore::new(512), 512, 16);
    let batch: Vec<BatchEntry> = (0..5u8)
      .map(|i| BatchEntry::upsert(vec![b'k', b'0' + i], vec![b'v'; 8], 1))
      .collect();
    store.put(&tbl, &batch).await.unwrap();
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 5);
    let switches = store.page_io().manifest_switch_num();
    let appends = store.page_io().manifest_append_num();
    let bytes = store
      .page_io()
      .read_manifest(&tbl)
      .await
      .unwrap()
      .expect("首写 snapshot");

    // position 大于全部 key：scan 为空 → 零写入（C++ 会重写右路径 + append）。
    assert_eq!(store.truncate_from(&tbl, b"zzzz").await.unwrap(), 0);
    assert_eq!(store.page_io().manifest_switch_num(), switches);
    assert_eq!(store.page_io().manifest_append_num(), appends);
    assert_eq!(
      store.page_io().read_manifest(&tbl).await.unwrap().unwrap(),
      bytes,
      "无匹配 truncate_from 不得触碰 manifest"
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 5);

    // 对照：有匹配的 truncate_from 删除 3 条并 append 一条 delta。
    assert_eq!(store.truncate_from(&tbl, b"k2").await.unwrap(), 3);
    assert_eq!(store.page_io().manifest_switch_num(), switches);
    assert_eq!(
      store.page_io().manifest_append_num(),
      appends + 1,
      "有匹配的删除批必须 append"
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
  });
}
