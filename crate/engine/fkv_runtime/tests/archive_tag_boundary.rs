//! sem-10：归档 tag 的字符集边界走公共 `Runtime` 控制面。
//!
//! C++ `BranchArchiveName` 对 tag 零字符集校验（中文 / `:` / 空格都直接拼进
//! `manifest_<branch>_<term>_<tag>`），空 tag 由后台任务自动生成微秒时间戳。
//! 这里锁定 Rust 公共入口的行为：C++ 风格 tag 能创建 / 列举 / 全局重开 / 全局
//! 删除；单表空 tag 自动生成时间戳（对齐 C++）；`.tmp` 后缀在写入面拒绝。

use std::path::PathBuf;

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_runtime::{ArchiveEntry, Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root);
  config.archive_num_retained = 10;
  config
}

#[test]
fn cpp_style_tags_flow_through_global_control_plane() {
  let root = TempRoot::new("archive_tag_boundary_global");
  let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
  pollster::block_on(async {
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

    // 中文 + `:` + 空格 + `_`：C++ 可直接写出，Rust 必须能建、能列、能恢复。
    let tag = "备份:prod_v1";
    let reply = runtime.global_archive(tag, 10).await.unwrap();
    assert_eq!(reply.total(), 2, "{reply:?}");
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    assert_eq!(reply.name, format!("manifest_main_1_{tag}"));
    assert_eq!(reply.tag, tag);
    assert_eq!(
      runtime.global_list_archive_tags("").await.unwrap(),
      vec![ArchiveEntry {
        tag: tag.to_owned(),
        term: 1,
      }]
    );
    // 单表列举同样可见（旧白名单时代这里能看到、reopen 却报 InvalidArgs）。
    let got: Vec<(String, u64)> = runtime
      .list_archive_tags(&tbl_li[0])
      .await
      .unwrap()
      .into_iter()
      .map(|entry| (entry.tag, entry.term))
      .collect();
    assert_eq!(got, vec![(tag.to_owned(), 1)]);

    // 归档后继续写：全局按 tag 重开必须命中两个分区并回到归档点。
    for tbl in &tbl_li {
      runtime
        .put(
          tbl,
          vec![BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
        )
        .await
        .unwrap();
    }
    let reply = runtime.global_reopen(tag).await.unwrap();
    assert_eq!(reply.total(), 2, "{reply:?}");
    assert_eq!(reply.ok_num, 2, "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    for tbl in &tbl_li {
      assert_eq!(runtime.get(tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
      assert_eq!(runtime.get(tbl, b"b").await.unwrap(), None);
    }

    // 全局删除：ok_num == total()，删完清单为空。
    let reply = runtime.global_delete_archive(tag).await.unwrap();
    assert_eq!(reply.total(), 2, "{reply:?}");
    assert_eq!(reply.ok_num, reply.total(), "{reply:?}");
    assert!(reply.failed_li.is_empty(), "{reply:?}");
    assert!(
      runtime
        .global_list_archive_tags("")
        .await
        .unwrap()
        .is_empty()
    );

    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn single_table_empty_tag_generates_timestamp_and_tmp_suffix_is_rejected() {
  let root = TempRoot::new("archive_tag_boundary_single");
  let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
  pollster::block_on(async {
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    // 单表空 tag：对齐 C++ `BackgroundWrite::CreateArchive` 的微秒时间戳，
    // 且回执名里带生成的 tag（定宽 20 位，供 prune 名字排序 = 时间排序）。
    let reply = runtime.create_archive(&tbl, "", 10).await.unwrap();
    let generated = reply
      .name
      .strip_prefix("manifest_main_1_")
      .expect("archive name should carry generated tag")
      .to_owned();
    assert_eq!(generated.len(), 20, "{generated}");
    assert!(generated.bytes().all(|byte| byte.is_ascii_digit()));
    assert_eq!(
      runtime.list_archive_tags(&tbl).await.unwrap().len(),
      1,
      "空 tag 生成的归档必须可列举"
    );

    // 空 tag 重开 = 重开活跃快照（不是归档选择）。
    runtime.reopen_archive(&tbl, "").await.unwrap();

    // `.tmp` 后缀是 manifest 整写中间态：写入面拒绝，避免产出自己列举不到的
    // 永久孤儿（旧实现会成功写盘但 list / reopen / prune 都看不见）。
    assert_eq!(
      runtime.create_archive(&tbl, "x.tmp", 10).await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      runtime.reopen_archive(&tbl, "x.tmp").await.unwrap_err(),
      KvError::InvalidArgs
    );
    let reply = runtime.global_archive("x.tmp", 10).await.unwrap();
    assert_eq!(reply.total(), 1, "{reply:?}");
    assert_eq!(reply.ok_num, 0, "{reply:?}");
    assert_eq!(reply.failed_li.len(), 1, "{reply:?}");
    assert!(!root.join("demo.1").join("manifest_main_1_x.tmp").exists());

    runtime.shutdown().await.unwrap();
  });
}
