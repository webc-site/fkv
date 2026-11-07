//! 归档生命周期控制面：全局一次性 tag、按 tag 重开 / 删除、全局删表按表名过滤、
//! 全局建分支的规范化 salt 名。
//!
//! 控制面语义：
//! - `HandleGlobalArchiveRequest` 空 tag 只生成一次并广播给全部分区，结果名
//!   通过 `ResultArchive` 返回；
//! - `ReopenRequest::SetTag` / `ArchiveRequest::Action::Delete` 的 per-table 语义；
//! - `HandleDropTableRequest` 按表名收集分区；
//! - `HandleGlobalCreateBranchRequest` 的 `normalized + "-" + %08x` 内部名。

use std::{
  fs::read_dir,
  path::{Path, PathBuf},
};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_runtime::{ArchiveEntry, Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: 1,
    root,
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
    archive_num_retained: 10,
  }
}

fn partition_dir(root: &Path, tbl: &TableIdent) -> PathBuf {
  root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
}

fn file_name_set(dir: &Path) -> Vec<String> {
  let mut name_li: Vec<String> = read_dir(dir)
    .unwrap()
    .flatten()
    .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
    .collect();
  name_li.sort();
  name_li
}

#[test]
fn global_archive_tag_reopen_delete_and_table_scoped_drop() {
  let root = TempRoot::new("archive_lifecycle");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
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

    // 1) 空 tag：只在全局层生成一次，两个分区写同名归档，名字回传给调用方。
    let reply = runtime.global_archive("", 10).await.unwrap();
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    assert!(!reply.tag.is_empty(), "{reply:?}");
    assert_eq!(reply.name, format!("manifest_main_1_{}", reply.tag));
    for tbl in &tbl_li {
      assert!(
        file_name_set(&partition_dir(root.path(), tbl)).contains(&reply.name),
        "partition {tbl} missing {}: {:?}",
        reply.name,
        file_name_set(&partition_dir(root.path(), tbl))
      );
    }
    // 两个分区的 (tag, term) 全等：全局列举按 C++ sort+unique 只回一条。
    assert_eq!(
      runtime.global_list_archive_tags("").await.unwrap(),
      vec![ArchiveEntry {
        tag: reply.tag.clone(),
        term: 1,
      }]
    );
    let tag = reply.tag.clone();

    // 2) 归档后继续写，活跃状态领先于归档。
    for tbl in &tbl_li {
      runtime
        .put(
          tbl,
          vec![BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
        )
        .await
        .unwrap();
      assert_eq!(runtime.get(tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    }

    // 3) 按 tag 全局重开：两个分区都回到归档点。
    let reply = runtime.global_reopen(&tag).await.unwrap();
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    for tbl in &tbl_li {
      assert_eq!(runtime.get(tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
      assert_eq!(runtime.get(tbl, b"b").await.unwrap(), None);
    }

    // 空 tag 的全局重开 = 重开活跃快照。
    let reply = runtime.global_reopen("").await.unwrap();
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert_eq!(
      runtime.get(&tbl_li[0], b"a").await.unwrap(),
      Some(b"A".to_vec())
    );

    // 4) 按 tag 删除：单分区删除 + 全局删除都可用；幂等；删完后该 tag 不再出现。
    assert_eq!(runtime.delete_archive(&tbl_li[0], &tag).await.unwrap(), 1);
    let reply = runtime.global_delete_archive(&tag).await.unwrap();
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    assert!(
      runtime
        .global_list_archive_tags(&tag)
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(runtime.delete_archive(&tbl_li[0], &tag).await.unwrap(), 0);
    assert_eq!(
      runtime.reopen_archive(&tbl_li[0], &tag).await.unwrap_err(),
      KvError::ResourceMissing
    );
    assert_eq!(
      runtime.global_delete_archive("").await.unwrap_err(),
      KvError::InvalidArgs
    );

    // 5) 全局建分支：逻辑名归一化 + 8 位 salt；调用方拿内部名。
    let reply = runtime
      .global_create_branch_with_salt("Snap", 0xabc)
      .await
      .unwrap();
    assert_eq!(reply.name, "snap-00000abc", "{reply:?}");
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    for tbl in &tbl_li {
      assert!(
        file_name_set(&partition_dir(root.path(), tbl))
          .contains(&"manifest_snap-00000abc_0".to_owned()),
        "partition {tbl} missing salted branch manifest"
      );
    }

    // 6) 全局删表按表名过滤：只删 demo，不碰 other。
    let other = TableIdent::new("other", 1).unwrap();
    runtime
      .put(
        &other,
        vec![BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 1)],
      )
      .await
      .unwrap();
    let reply = runtime.global_drop_table("demo").await.unwrap();
    assert_eq!(reply.total(), 2, "{reply:?}");
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert!(
      partition_dir(root.path(), &other).exists(),
      "other partition was dropped"
    );
    assert_eq!(
      runtime.get(&other, b"x").await.unwrap(),
      Some(b"X".to_vec())
    );

    runtime.shutdown().await.unwrap();
  });
}

/// 0 个目标 / 全部 detach 时结果名仍按 attach 身份构造（C++ `result_archive_`
/// 在列举分区前赋值）；`global_delete_archive` 与 create 同路径。
#[test]
fn empty_target_global_reply_uses_attach_identity() {
  let root = TempRoot::new("archive_lifecycle");
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 2,
      ..config(root.path().to_path_buf())
    })
    .unwrap();

    // 未 attach：branch 回落 main、term 回落 0（C++ `ShardTerm` 语义）。
    let reply = runtime.global_archive("pre", 0).await.unwrap();
    assert_eq!(reply.total(), 0, "{reply:?}");
    assert_eq!(reply.tag, "pre");
    assert_eq!(reply.name, "manifest_main_0_pre");

    // attach 后：branch 归一化（MAIN -> main）+ attach term。
    runtime.attach_shard(0, "MAIN", 7).await.unwrap();
    let reply = runtime.global_archive("", 0).await.unwrap();
    assert_eq!(reply.total(), 0, "{reply:?}");
    assert!(!reply.tag.is_empty());
    assert_eq!(reply.name, format!("manifest_main_7_{}", reply.tag));

    // 多 shard 身份不一致时按最低 shard_id，结果稳定不随 fanout 顺序漂移。
    runtime.attach_shard(1, "feature", 9).await.unwrap();
    let reply = runtime.global_archive("again", 0).await.unwrap();
    assert_eq!(reply.name, "manifest_main_7_again");

    // 删归档同走 `HandleGlobalArchiveRequest`，name / tag 一并返回。
    let reply = runtime.global_delete_archive("again").await.unwrap();
    assert_eq!(reply.total(), 0, "{reply:?}");
    assert_eq!(reply.tag, "again");
    assert_eq!(reply.name, "manifest_main_7_again");

    // 全部 detach 后回落到 main/0。
    runtime.detach_shard(0).await.unwrap();
    runtime.detach_shard(1).await.unwrap();
    let reply = runtime.global_archive("post", 0).await.unwrap();
    assert_eq!(reply.name, "manifest_main_0_post");

    // 全局建分支的名字与目标集合无关。
    let reply = runtime
      .global_create_branch_with_salt("Snap", 0xabc)
      .await
      .unwrap();
    assert_eq!(reply.total(), 0, "{reply:?}");
    assert_eq!(reply.name, "snap-00000abc");

    runtime.shutdown().await.unwrap();
  });
}

/// sem-09：含 `.` 的表名端到端可用。目录名 `{tbl}.{partition}` 按最后一个
/// `.` 切分（`my.table.0` -> `{my.table, 0}`），`global_archive` /
/// `global_reopen` / `global_drop_table` 的目标集合都必须命中该分区。
#[test]
fn dotted_table_name_is_visible_to_global_control_plane() {
  let root = TempRoot::new("archive_dotted_table");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();

    let tbl = TableIdent::new("my.table", 0).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));
    assert!(
      partition_dir(root.path(), &tbl).is_dir(),
      "my.table.0 未落盘"
    );

    // 归档目标集合按目录名枚举：`my.table.0` 必须解析为 `{my.table, 0}`。
    let reply = runtime.global_archive("dotted", 1).await.unwrap();
    assert_eq!(reply.total(), 1, "{reply:?}");
    assert_eq!(reply.ok_num, 1, "{reply:?}");
    assert_eq!(reply.tag, "dotted");

    // 归档后继续写，按 tag 全局重开同样必须命中该分区。
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"after".to_vec(), b"A".to_vec(), 2)],
      )
      .await
      .unwrap();
    let reply = runtime.global_reopen("dotted").await.unwrap();
    assert_eq!(reply.total(), 1, "{reply:?}");
    assert_eq!(reply.ok_num, 1, "{reply:?}");
    assert_eq!(runtime.get(&tbl, b"after").await.unwrap(), None);
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), Some(b"v".to_vec()));

    // 全局删表按表名精确匹配，`my.table` 的目录必须被枚举并删除。
    let reply = runtime.global_drop_table("my.table").await.unwrap();
    assert_eq!(reply.total(), 1, "{reply:?}");
    assert_eq!(reply.ok_num, 1, "{reply:?}");
    assert_eq!(runtime.get(&tbl, b"k").await.unwrap(), None);

    runtime.shutdown().await.unwrap();
  });
}
