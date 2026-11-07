//! `fkv_io_sim` 集成测试：掉电语义、确定性、故障注入、六组 trait 往返。

use fkv_error::KvError;
use fkv_io::{FileIo, FixedSegmentIo, ManifestIo, PageIo, SegmentIo, TermIo};
use fkv_io_sim::{Fault, OpKind, OpRecord, SimStore, Snapshot};
use fkv_types::TableIdent;

fn tbl() -> TableIdent {
  TableIdent::new("demo", 1).unwrap()
}

#[test]
fn crash_drops_unsynced_and_keeps_synced() {
  pollster::block_on(async {
    let store = SimStore::new(8);
    let tbl = tbl();

    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(&dst, b"12345678", "写入 crash 前应可见（page cache 语义）");

    store.crash();
    let mut lost = [0u8; 8];
    assert_eq!(
      store.read_page(&tbl, 0, &mut lost).await.unwrap_err(),
      KvError::ResourceMissing,
      "未 sync 的页写必须在 crash 后丢失"
    );

    store.write_page(&tbl, 0, b"abcdefgh").await.unwrap();
    store.sync_data(&tbl).await.unwrap();
    store.crash();
    let mut kept = [0u8; 8];
    store.read_page(&tbl, 0, &mut kept).await.unwrap();
    assert_eq!(&kept, b"abcdefgh", "sync 后的页写必须在 crash 后保留");
  });
}

/// manifest 写路径自带 fsync（compio：write/fdatasync/rename + 目录 fsync），
/// 写成功即 durable，不依赖 `sync_data`；`sync_data` 只刷 page / segment。
///
/// 回退即失败：旧实现把 manifest 留在 volatile、由 `sync_data` 提升，等于把
/// 「manifest 漏 fsync」的后端缺陷洗成通过。
#[test]
fn manifest_writes_are_durable_without_sync_data() {
  pollster::block_on(async {
    let store = SimStore::new(8);
    let tbl = tbl();

    store.switch_manifest(&tbl, b"v1").await.unwrap();
    store.crash();
    assert_eq!(
      store.read_manifest(&tbl).await.unwrap().unwrap(),
      b"v1",
      "manifest 写成功即 durable"
    );

    // 数据类写入仍然要 `sync_data` 才抗掉电。
    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    store.crash();
    let mut lost = [0u8; 8];
    assert_eq!(
      store.read_page(&tbl, 0, &mut lost).await.unwrap_err(),
      KvError::ResourceMissing,
      "sync_data 只刷数据类，未落的页写必须丢"
    );

    // 删除同样即时 durable。
    store.drop_manifest(&tbl).await.unwrap();
    store.crash();
    assert!(store.read_manifest(&tbl).await.unwrap().is_none());
  });
}

fn deterministic_run(seed: u64) -> (Vec<OpRecord>, Snapshot) {
  pollster::block_on(async {
    let store = SimStore::new(8).with_reorder_seed(seed);
    let tbl = tbl();
    for index in 0..4u64 {
      let page = [b'a' + index as u8; 8];
      store.write_page(&tbl, index, &page).await.unwrap();
    }
    store.sync_data(&tbl).await.unwrap();
    store.crash();

    store.write_page(&tbl, 1, b"ZZZZZZZZ").await.unwrap();
    for index in 4..6u64 {
      store.write_page(&tbl, index, b"12345678").await.unwrap();
    }
    store.sync_data(&tbl).await.unwrap();
    (store.op_log_li(), store.snapshot())
  })
}

#[test]
fn reorder_seed_is_deterministic() {
  let (log_a, snap_a) = deterministic_run(42);
  let (log_b, snap_b) = deterministic_run(42);
  assert_eq!(log_a, log_b, "同 seed 两次 op_log 必须逐项相等");
  assert_eq!(snap_a, snap_b, "同 seed 两次盘面必须逐字节相等");

  // 不同 seed 只影响刷写顺序，不影响最终盘面。
  let (_, snap_c) = deterministic_run(7);
  assert_eq!(snap_c, snap_a, "reorder 只改顺序，不改最终状态");

  // flush 顺序被记录，使 reorder 可观察。
  assert!(
    log_a
      .iter()
      .any(|record| record.op.kind() == OpKind::FlushPage)
  );
}

#[test]
fn fault_returns_error_and_keeps_state() {
  pollster::block_on(async {
    let store = SimStore::new(8).with_fault(Fault {
      op: OpKind::WritePage,
      at: 2,
      err: KvError::IoFail,
    });
    let tbl = tbl();

    store.write_page(&tbl, 0, b"00000000").await.unwrap();
    assert_eq!(
      store.write_page(&tbl, 1, b"11111111").await.unwrap_err(),
      KvError::IoFail
    );
    let mut buf = [0u8; 8];
    assert_eq!(
      store.read_page(&tbl, 1, &mut buf).await.unwrap_err(),
      KvError::ResourceMissing,
      "故障命中那次不得改动状态"
    );
    store.write_page(&tbl, 2, b"22222222").await.unwrap();
    assert_eq!(store.op_count(OpKind::WritePage), 3);

    let mut kept = [0u8; 8];
    store.read_page(&tbl, 0, &mut kept).await.unwrap();
    assert_eq!(&kept, b"00000000");
    store.read_page(&tbl, 2, &mut kept).await.unwrap();
    assert_eq!(&kept, b"22222222");
  });
}

/// FlushPage 故障必须在状态迁移前判定：命中那次不得把页写进 durable。
#[test]
fn flush_page_fault_leaves_no_half_commit() {
  pollster::block_on(async {
    let store = SimStore::new(8).with_fault(Fault {
      op: OpKind::FlushPage,
      at: 1,
      err: KvError::IoFail,
    });
    let tbl = tbl();
    store.write_page(&tbl, 0, b"00000000").await.unwrap();
    store.write_page(&tbl, 1, b"11111111").await.unwrap();

    assert_eq!(store.sync_data(&tbl).await.unwrap_err(), KvError::IoFail);
    assert_eq!(store.op_count(OpKind::FlushPage), 1);

    // volatile 仍可见（page cache 语义）。
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(&dst, b"00000000");

    // 掉电后两页都必须丢失：故障前不得已提交第一页。
    store.crash();
    let mut lost = [0u8; 8];
    assert_eq!(
      store.read_page(&tbl, 0, &mut lost).await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert_eq!(
      store.read_page(&tbl, 1, &mut lost).await.unwrap_err(),
      KvError::ResourceMissing
    );
  });
}

#[test]
fn delay_is_recorded_in_op_log() {
  pollster::block_on(async {
    let store = SimStore::new(8).with_delay(OpKind::WritePage, 7);
    let tbl = tbl();
    store.write_page(&tbl, 0, b"00000000").await.unwrap();

    let log = store.op_log_li();
    let last = log.last().unwrap();
    assert_eq!(last.op.kind(), OpKind::WritePage);
    assert_eq!(last.delay, 7);
    assert_eq!(last.clock, 1);
    assert_eq!(store.clock(), 1);
  });
}

#[test]
fn manifest_roundtrip_matches_mem_semantics() {
  pollster::block_on(async {
    let store = SimStore::new(8);
    let tbl = tbl();
    store.append_manifest(&tbl, b"abc").await.unwrap();
    store.append_manifest(&tbl, b"def").await.unwrap();
    assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"abcdef");
    store.reset_manifest(&tbl, 2).await.unwrap();
    assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"ab");
    store.switch_manifest(&tbl, b"xyz").await.unwrap();
    assert_eq!(store.read_manifest(&tbl).await.unwrap().unwrap(), b"xyz");
    store.drop_manifest(&tbl).await.unwrap();
    assert!(store.read_manifest(&tbl).await.unwrap().is_none());
  });
}

#[test]
fn manifest_file_roundtrip() {
  pollster::block_on(async {
    let store = SimStore::new(8);
    let tbl = tbl();
    store
      .write_manifest_file(&tbl, "manifest_main_0", b"one")
      .await
      .unwrap();
    store
      .append_manifest_file(&tbl, "manifest_main_0", b"two")
      .await
      .unwrap();
    store
      .write_manifest_file(&tbl, "manifest_b_0", b"branch")
      .await
      .unwrap();
    assert_eq!(
      store.list_manifest_file_name_li(&tbl).await.unwrap(),
      vec!["manifest_b_0".to_owned(), "manifest_main_0".to_owned()]
    );
    assert_eq!(
      store
        .read_manifest_file(&tbl, "manifest_main_0")
        .await
        .unwrap()
        .unwrap(),
      b"onetwo"
    );
    store
      .reset_manifest_file(&tbl, "manifest_main_0", 3)
      .await
      .unwrap();
    assert_eq!(
      store
        .read_manifest_file(&tbl, "manifest_main_0")
        .await
        .unwrap()
        .unwrap(),
      b"one"
    );
    store
      .remove_manifest_file(&tbl, "manifest_main_0")
      .await
      .unwrap();
    assert!(
      store
        .read_manifest_file(&tbl, "manifest_main_0")
        .await
        .unwrap()
        .is_none()
    );
  });
}

#[test]
fn term_roundtrip() {
  pollster::block_on(async {
    let store = SimStore::new(8);
    assert_eq!(store.read_term("main", 1).await.unwrap(), None);
    store.upsert_term("main", 1, 5).await.unwrap();
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(5));
    assert_eq!(
      store.upsert_term("main", 1, 4).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    // term 立即 durable：crash 不丢。
    store.crash();
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(5));
  });
}

#[test]
fn segment_roundtrip_and_crash() {
  pollster::block_on(async {
    let store = SimStore::new(8).with_segment_config(16, 2);
    let tbl = tbl();
    store.write_segment(&tbl, 3, b"hello").await.unwrap();
    let mut dst = [0xffu8; 8];
    store.read_segment(&tbl, 3, &mut dst).await.unwrap();
    assert_eq!(&dst, b"hello\0\0\0");

    store.crash();
    assert_eq!(
      store.read_segment(&tbl, 3, &mut dst).await.unwrap_err(),
      KvError::ResourceMissing
    );

    store.write_segment(&tbl, 3, b"hello").await.unwrap();
    store.sync_data(&tbl).await.unwrap();
    store.crash();
    store.read_segment(&tbl, 3, &mut dst).await.unwrap();
    assert_eq!(&dst, b"hello\0\0\0");
  });
}

#[test]
fn fixed_segment_matches_segment() {
  pollster::block_on(async {
    let store = SimStore::new(8).with_segment_config(16, 2);
    let tbl = tbl();
    let mut buf = store.register_fixed_buffer(16).unwrap();
    buf.as_mut_slice().expect("无在飞 fixed op")[4..9].copy_from_slice(b"world");
    store
      .write_segment_fixed(&tbl, 1, &buf, 4, 5)
      .await
      .unwrap();

    let mut seg = [0u8; 5];
    store.read_segment(&tbl, 1, &mut seg).await.unwrap();
    assert_eq!(&seg, b"world");

    buf.as_mut_slice().expect("无在飞 fixed op")[4..9].fill(0);
    store
      .read_segment_fixed(&tbl, 1, &mut buf, 4, 5)
      .await
      .unwrap();
    assert_eq!(&buf.as_slice()[4..9], b"world");
  });
}

#[test]
fn file_roundtrip_and_remove() {
  pollster::block_on(async {
    let store = SimStore::new(8).with_pages_per_file(8, 4);
    let tbl = tbl();
    store.write_file(&tbl, 0, &[1u8; 16]).await.unwrap();
    assert_eq!(
      store.read_file(&tbl, 0).await.unwrap().unwrap(),
      vec![1u8; 16]
    );
    assert_eq!(store.file_size(&tbl, 0).await.unwrap(), Some(16));
    assert_eq!(store.list_file_id_li(&tbl).await.unwrap(), vec![0]);

    store.write_file(&tbl, 5, &[7u8; 8]).await.unwrap();
    assert_eq!(store.list_file_id_li(&tbl).await.unwrap(), vec![0, 5]);
    assert!(store.read_file(&tbl, 9).await.unwrap().is_none());

    store.remove_file(&tbl, 0).await.unwrap();
    assert!(store.read_file(&tbl, 0).await.unwrap().is_none());
    assert_eq!(store.list_file_id_li(&tbl).await.unwrap(), vec![5]);
  });
}

#[test]
fn fault_at_zero_never_fires() {
  pollster::block_on(async {
    // `at` 从 1 起算，0 永不命中。
    let store = SimStore::new(8).with_fault(Fault {
      op: OpKind::ReadPage,
      at: 0,
      err: KvError::IoFail,
    });
    let tbl = tbl();
    store.write_page(&tbl, 0, b"00000000").await.unwrap();
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 0, &mut dst).await.unwrap();
  });
}

#[test]
fn page_write_rejects_wrong_len() {
  pollster::block_on(async {
    let store = SimStore::new(8);
    let tbl = tbl();
    assert_eq!(
      store.write_page(&tbl, 0, b"short").await.unwrap_err(),
      KvError::InvalidArgs
    );
  });
}

/// 旁路文件（如 segment 完成标记）必须可枚举、可删除，且删除在 `sync_data`
/// 后仍是墓碑语义（不能因 durable 层残留被复活）。
#[test]
fn sidecar_file_li_and_remove_roundtrip() {
  pollster::block_on(async {
    let store = SimStore::new(8);
    let tbl = tbl();

    store
      .write_manifest_file(&tbl, "manifest_main_0", b"m")
      .await
      .unwrap();
    store
      .write_manifest_file(&tbl, "segcomplete_7", b"magic")
      .await
      .unwrap();
    store
      .write_manifest_file(&tbl, "manifest_feature_3", b"b")
      .await
      .unwrap();
    assert_eq!(
      store.list_sidecar_file_name_li(&tbl).await.unwrap(),
      vec!["segcomplete_7".to_owned()],
      "只列出非 manifest 命名的旁路文件"
    );
    assert_eq!(
      store.list_manifest_file_name_li(&tbl).await.unwrap(),
      vec![
        "manifest_feature_3".to_owned(),
        "manifest_main_0".to_owned()
      ],
      "旁路名不得混入 manifest 名单（GC 会逐名 replay）"
    );

    store.sync_data(&tbl).await.unwrap();
    store
      .remove_sidecar_file_name(&tbl, "segcomplete_7")
      .await
      .unwrap();
    store.sync_data(&tbl).await.unwrap();
    assert!(
      store
        .list_sidecar_file_name_li(&tbl)
        .await
        .unwrap()
        .is_empty(),
      "删除后不得被 durable 层复活"
    );
  });
}
