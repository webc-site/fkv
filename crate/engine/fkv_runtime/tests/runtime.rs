use std::path::PathBuf;

use fkv_api::Request;
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_metrics::Op;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn config(root: PathBuf, num_threads: u16) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root);
  config.num_threads = num_threads;
  config
}

async fn attach_all(runtime: &Runtime, branch: &str, term: u64) {
  for shard_id in 0..u32::from(runtime.num_threads()) {
    runtime.attach_shard(shard_id, branch, term).await.unwrap();
  }
}

#[test]
fn thread_per_core_put_get_scan_reopen() {
  let root = TempRoot::new("runtime");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf(), 2)).unwrap();
    attach_all(&runtime, "main", 1).await;
    let tbl_a = TableIdent::new("demo", 1).unwrap();
    let tbl_b = TableIdent::new("demo", 2).unwrap();
    runtime
      .put(
        &tbl_a,
        vec![
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    runtime
      .put(
        &tbl_b,
        vec![BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(
      runtime.get(&tbl_a, b"a").await.unwrap(),
      Some(b"A".to_vec())
    );
    assert_eq!(runtime.scan(&tbl_a, b"", b"", 0).await.unwrap().len(), 2);
    assert_eq!(
      runtime.get(&tbl_b, b"x").await.unwrap(),
      Some(b"X".to_vec())
    );
    runtime.shutdown().await.unwrap();

    let runtime = Runtime::start(config(root.path().to_path_buf(), 2)).unwrap();
    attach_all(&runtime, "main", 1).await;
    runtime.reopen(&tbl_a).await.unwrap();
    runtime.reopen(&tbl_b).await.unwrap();
    assert_eq!(
      runtime.get(&tbl_a, b"b").await.unwrap(),
      Some(b"B".to_vec())
    );
    assert_eq!(
      runtime.get(&tbl_b, b"x").await.unwrap(),
      Some(b"X".to_vec())
    );
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn detach_and_term_validation() {
  let root = TempRoot::new("runtime");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf(), 1)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::NotRunning
    );
    runtime.attach_shard(0, "main", 5).await.unwrap();
    runtime
      .put_with_term(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        5,
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 1)],
          4,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    // 整 shard 已过期：无 term 的普通读也快速失败。
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::ExpiredTerm
    );
    assert_eq!(
      runtime.attach_shard(0, "main", 4).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    // attach 失败必须回滚本层 attached 视图：归档目标集合不再包含这个已
    // `Expired` 的 shard，全局结果名也不再沿用已失效的 `(main, 5)`。
    let reply = runtime.global_archive("rollback", 1).await.unwrap();
    assert_eq!(
      reply.ok_num, 0,
      "已 Expired 的 shard 不应归档成功: {reply:?}"
    );
    assert_eq!(reply.name, "manifest_main_0_rollback", "{reply:?}");

    runtime.detach_shard(0).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::NotRunning
    );
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A3".to_vec(), 1)],
          5,
        )
        .await
        .unwrap_err(),
      KvError::NotRunning
    );

    runtime.attach_shard(0, "main", 6).await.unwrap();
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A3".to_vec(), 1)],
          5,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    // 旧 term 请求已把整个 shard 标记过期：需重新 attach 清除。
    runtime.attach_shard(0, "main", 6).await.unwrap();
    runtime
      .put_with_term(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A3".to_vec(), 1)],
        6,
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A3".to_vec()));

    // 换 branch 后 term 重新计数；先为 other table 创建 branch manifest。
    // 已托管时换 branch 属于非法转换，必须先 detach（对齐 C++ AttachShard 要求 Detached）。
    let tbl_other = TableIdent::new("other", 1).unwrap();
    // 空表不会创建 branch manifest（对齐 C++ NotFound 直接成功）；先写一条
    // 数据，让 branch 有可继承的 mapping。
    runtime
      .put(
        &tbl_other,
        vec![BatchEntry::upsert(b"seed".to_vec(), b"S".to_vec(), 1)],
      )
      .await
      .unwrap();
    runtime.create_branch(&tbl_other, "other").await.unwrap();
    assert_eq!(
      runtime.attach_shard(0, "other", 1).await.unwrap_err(),
      KvError::InvalidArgs
    );
    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "other", 1).await.unwrap();
    runtime
      .put_with_term(
        &tbl_other,
        vec![BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 1)],
        1,
      )
      .await
      .unwrap();
    assert_eq!(
      runtime.get(&tbl_other, b"x").await.unwrap(),
      Some(b"X".to_vec())
    );
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn put_with_term_expired_clears_attach_view() {
  let root = TempRoot::new("runtime");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf(), 1)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();
    // 旧 term 写：shard 侧把 lifecycle 置为 `Expired`，本层视图必须同步清空。
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
          4,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    // 视图未清时这里会用旧身份 `(main, 5)` 构造 `manifest_main_5_...`。
    let reply = runtime.global_archive("cleanup", 1).await.unwrap();
    assert_eq!(
      reply.ok_num, 0,
      "已 Expired 的 shard 不应归档成功: {reply:?}"
    );
    assert_eq!(reply.name, "manifest_main_0_cleanup", "{reply:?}");
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn direct_request_expired_clears_attach_view() {
  let root = TempRoot::new("runtime");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf(), 1)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 5).await.unwrap();
    // 不经 `put_with_term`，直接用公开 `Request::Put { term }` 走 `request`：
    // 统一收尾同样要在 `ExpiredTerm` 时清掉本层 attach 视图。
    assert_eq!(
      runtime
        .request(
          &tbl,
          Request::Put {
            tbl: tbl.clone(),
            batch_li: vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
            term: Some(4),
          },
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    // 视图未清时这里会用旧身份 `(main, 5)` 构造 `manifest_main_5_...`。
    let reply = runtime.global_archive("direct", 1).await.unwrap();
    assert_eq!(
      reply.ok_num, 0,
      "已 Expired 的 shard 不应归档成功: {reply:?}"
    );
    assert_eq!(reply.name, "manifest_main_0_direct", "{reply:?}");
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn truncate_and_drop_requests() {
  let root = TempRoot::new("runtime");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf(), 1)).unwrap();
    attach_all(&runtime, "main", 1).await;
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    runtime.truncate(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), None);
    assert!(runtime.scan(&tbl, b"", b"", 0).await.unwrap().is_empty());

    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 2)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"x").await.unwrap(), Some(b"X".to_vec()));
    runtime.drop_table(&tbl).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"x").await.unwrap(), None);

    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"z".to_vec(), b"Z".to_vec(), 3)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"z").await.unwrap(), Some(b"Z".to_vec()));
    runtime.shutdown().await.unwrap();
  });
}

#[test]
fn metrics_track_requests_latency_and_io() {
  let root = TempRoot::new("runtime");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf(), 2)).unwrap();
    attach_all(&runtime, "main", 1).await;
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    runtime.scan(&tbl, b"", b"", 0).await.unwrap();

    let metrics = runtime.metrics();
    assert_eq!(metrics.shard_li.len(), 2);
    assert_eq!(metrics.total_requests(Op::Put), 1);
    assert_eq!(metrics.total_requests(Op::Scan), 1);
    assert_eq!(metrics.total_errors(Op::Put), 0);
    let put_latency: u64 = metrics
      .shard_li
      .iter()
      .map(|shard| shard.op(Op::Put).latency.count)
      .sum();
    assert_eq!(put_latency, 1);
    assert!(
      metrics
        .shard_li
        .iter()
        .map(|shard| shard.page_writes)
        .sum::<u64>()
        > 0
    );
    assert!(
      metrics
        .shard_li
        .iter()
        .map(|shard| shard.bytes_written)
        .sum::<u64>()
        > 0
    );

    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 2)],
          2,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    let metrics = runtime.metrics();
    assert_eq!(metrics.total_errors(Op::Put), 1);
    assert_eq!(metrics.total_requests(Op::Put), 2);
    runtime.shutdown().await.unwrap();
  });
}

/// 控制面请求按 shard 定位，必须走 `attach_shard` / `detach_shard` / `shutdown`；
/// 数据面 `request*` 入口拒绝它们，避免绕过 attach 身份记录与按 `tbl` 清错 shard。
#[test]
fn data_request_rejects_control_plane() {
  let root = TempRoot::new("runtime");
  pollster::block_on(async {
    let runtime = Runtime::start_mem(config(root.path().to_path_buf(), 1)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    for req in [
      Request::AttachShard {
        branch: "main".to_owned(),
        shard_id: 0,
        term: 1,
      },
      Request::DetachShard { shard_id: 0 },
      Request::Shutdown,
    ] {
      assert_eq!(
        runtime.request(&tbl, req).await.unwrap_err(),
        KvError::InvalidArgs
      );
    }
    // 被拒绝的 attach 没有生效：shard 仍未托管。
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap_err(),
      KvError::NotRunning
    );
    runtime.shutdown().await.unwrap();
  });
}
