//! per-table 请求面：`Prewarm` / `ListArchiveTags` / `ListObject` /
//! `ListStandbyPartition` 的 Runtime 便捷入口与模式守卫。
//!
//! 本用例只跑 Local：Prewarm / ListObject / ListStandbyPartition 必须显式回
//! `InvalidArgs`（不能静默成功或返回空结果），ListArchiveTags 必须能列到
//! `CreateArchive` 写出的 `(tag, term)`。

use std::{path::PathBuf, time::Duration};

use fkv_api::{ArchiveTagInfo, Request};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
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

#[test]
fn local_rejects_cloud_only_requests_and_lists_archive_tags() {
  let root = TempRoot::new("per_table_ext");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf())).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();

    let tbl = TableIdent::new("demo", 1).unwrap();
    // 未写过的表：列举空清单，不因「表未打开」报错，也不创建空表。
    assert!(runtime.list_archive_tags(&tbl).await.unwrap().is_empty());

    // 三个 Cloud/Standby 专属请求在 Local 下必须显式拒绝。
    assert_eq!(
      runtime.prewarm(&tbl).await.unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      runtime
        .list_object(&tbl, "demo.1/", None, 100, false)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
    assert_eq!(
      runtime.list_standby_partition().await.unwrap_err(),
      KvError::InvalidArgs
    );
    // 三个请求都没有触碰该分区：不懒建空表、不落任何本地文件。
    assert!(
      !root
        .join(format!("{}.{}", tbl.tbl_name, tbl.partition_id))
        .exists(),
      "control-plane requests must not create an empty partition"
    );

    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();
    runtime.create_archive(&tbl, "v2", 0).await.unwrap();
    runtime.create_archive(&tbl, "v1", 0).await.unwrap();

    assert_eq!(
      runtime.list_archive_tags(&tbl).await.unwrap(),
      vec![
        ArchiveTagInfo {
          tag: "v1".to_owned(),
          term: 1,
        },
        ArchiveTagInfo {
          tag: "v2".to_owned(),
          term: 1,
        },
      ]
    );

    runtime.shutdown().await.unwrap();
  });
}

/// `ListStandbyPartition` 没有 `tbl`：必须按控制面在路由前拒绝，而不是按调用方
/// `tbl` 落到某个 shard。
///
/// 用例让调用方 `tbl` 归到已 detach 的 shard 1：按 `tbl` 路由会拿到
/// `NotRunning`，控制面拒绝则是 `InvalidArgs`。回退 `is_control_plane`（漏掉该
/// 请求）时本用例拿到 `NotRunning`。
#[test]
fn control_plane_list_standby_partition_rejected_before_routing() {
  let root = TempRoot::new("per_table_ext");
  pollster::block_on(async {
    let mut config = config(root.path().to_path_buf());
    config.num_threads = 2;
    let runtime = Runtime::start(config).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    assert_eq!(tbl.shard_index(2), 1, "用例依赖该分区归 shard 1");
    runtime.attach_shard(0, "main", 1).await.unwrap();
    // shard 1 未 attach：按 tbl 路由会回 NotRunning。
    assert_eq!(
      runtime
        .request(&tbl, Request::ListStandbyPartition)
        .await
        .unwrap_err(),
      KvError::InvalidArgs,
      "控制面请求必须在路由前被拒绝"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// `ListStandbyPartition` 必须走带 deadline 的观测路由：master 卡死时 shard 侧
/// 的 `ssh` / `ls` 会同步阻塞，调用方必须有超时兜底而不能永久挂起。
///
/// 用 `with_request_timeout(Duration::ZERO)` 把 deadline 压到立即过期：Local 的
/// shard 本来会回 `InvalidArgs`。回退 `control_route(0).request(...)`（无
/// deadline）时本用例拿到 `InvalidArgs` 而不是 `Timeout`，直接失败。
#[test]
fn list_standby_partition_honours_request_deadline() {
  let root = TempRoot::new("per_table_ext");
  pollster::block_on(async {
    let runtime = Runtime::start(config(root.path().to_path_buf()))
      .unwrap()
      .with_request_timeout(Duration::ZERO);
    assert_eq!(
      runtime.list_standby_partition().await.unwrap_err(),
      KvError::Timeout,
      "观测请求必须带 deadline，回退无 deadline 的路由会拿到 InvalidArgs"
    );
    runtime.shutdown().await.unwrap();
  });
}
