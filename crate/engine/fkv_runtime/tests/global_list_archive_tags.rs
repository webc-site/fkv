use std::{
  env::temp_dir,
  fs::{create_dir_all, read_dir, remove_dir_all},
  path::{Path, PathBuf},
  process::id,
  time::{SystemTime, UNIX_EPOCH},
};

use fkv_btree::BatchEntry;
use fkv_runtime::{ArchiveEntry, Runtime, RuntimeConfig};
use fkv_types::{TableIdent, parse_manifest_file_name};

fn temp_root() -> PathBuf {
  let root = temp_dir().join(format!(
    "fkv_global_list_archive_tags_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  ));
  let _ = remove_dir_all(&root);
  create_dir_all(&root).unwrap();
  root
}

/// 直接从盘面文件名推出的归档条目（真实写入方产出的名字，不经过 runtime 列举逻辑）。
fn disk_entry_li(root: &Path, tbl: &TableIdent) -> Vec<ArchiveEntry> {
  let mut entry_li: Vec<ArchiveEntry> =
    read_dir(root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id)))
      .unwrap()
      .flatten()
      .filter_map(|entry| {
        let name = entry.file_name();
        let (_, term, tag) = parse_manifest_file_name(name.to_str()?)?;
        Some(ArchiveEntry {
          tag: tag?.to_owned(),
          term,
        })
      })
      .collect();
  entry_li.sort();
  entry_li
}

/// 端到端：真实写路径归档两个分区 → 全局列举聚合出全部归档条目。
///
/// 校验的是跨 crate 契约：写方（`fkv_store::create_archive`）产出的文件布局与
/// 名字格式，必须能被列举侧（本 crate）解析出来；两侧分叉时断言即失败。
#[test]
fn global_list_archive_tags_aggregates_real_archives() {
  let root = temp_root();
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.clone(),
      page_size: 512,
      restart_interval: 16,
      pages_per_file_shift: 18,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      queue_size: 64,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 5,
    })
    .unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();

    let tbl_li = [
      TableIdent::new("demo", 1).unwrap(),
      TableIdent::new("demo", 2).unwrap(),
    ];
    for tbl in &tbl_li {
      runtime
        .put(
          tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        )
        .await
        .unwrap();
    }

    // 归档前：盘上没有归档条目。
    assert!(
      runtime
        .global_list_archive_tag_li("")
        .await
        .unwrap()
        .is_empty()
    );

    for tag in ["001", "002"] {
      let reply = runtime.global_archive(tag, 5).await.unwrap();
      assert_eq!(reply.ok_num, 2, "{reply:?}");
      assert!(reply.failed_li.is_empty(), "{reply:?}");
    }

    // 两个分区各 2 份归档，但两分区 (tag, term) 全等 → 按 C++ `sort`+`unique`
    // 只回 2 条；顺序 = (tag, term)。
    let mut expect_li: Vec<ArchiveEntry> = tbl_li
      .iter()
      .flat_map(|tbl| disk_entry_li(&root, tbl))
      .collect();
    expect_li.sort();
    expect_li.dedup();
    assert_eq!(expect_li.len(), 2, "{expect_li:?}");
    assert_eq!(
      expect_li
        .iter()
        .map(|entry| entry.tag.as_str())
        .collect::<Vec<_>>(),
      vec!["001", "002"],
      "{expect_li:?}"
    );
    assert_eq!(
      runtime.global_list_archive_tag_li("").await.unwrap(),
      expect_li
    );
    assert_eq!(
      runtime
        .global_list_archive_tag_li("002")
        .await
        .unwrap()
        .len(),
      1
    );
    assert!(
      runtime
        .global_list_archive_tag_li("009")
        .await
        .unwrap()
        .is_empty()
    );

    runtime.shutdown().await.unwrap();
    let _ = remove_dir_all(&root);
  });
}
