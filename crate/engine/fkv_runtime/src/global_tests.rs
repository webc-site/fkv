use std::{
  fs,
  path::{Path, PathBuf},
  sync::Arc,
};

use fkv_cloud::CacheGroup;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_shard::{Backend, CloudBackend};
use fkv_testkit::TempRoot;

use crate::{
  ArchiveEntry, Runtime, RuntimeConfig,
  backend::backend_of,
  cloud_env::{CloudStoreConfig, open_runtime_cloud_store},
  config::write_limits,
};

fn temp_root(tag: &str) -> TempRoot {
  TempRoot::new(&format!("runtime_global_{tag}"))
}

fn config(root: PathBuf, threads: u16) -> RuntimeConfig {
  RuntimeConfig {
    num_threads: threads,
    root,
    page_size: 512,
    restart_interval: 16,
    pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    archive_interval_secs: 0,
    archive_num_retained: 100,
    queue_size: 64,
  }
}

/// `write_limits` 是 KvOption → ShardConfig 的写 IO 投影：配置值原样透传。
/// None 路径落到 `fkv_config` 默认常量（唯一来源），无需跨 crate 守卫。
#[test]
fn write_limits_project_config_overrides() {
  let option = KvOption {
    max_inflight_write: 7,
    max_write_batch_pages: 3,
    ..KvOption::default()
  };
  assert_eq!(write_limits(Some(&option)), (7, 3));
}

/// `backend_of`：Cloud 的共享预算组上限取 `local_space_limit`；非 S3 兼容
/// provider 显式拒绝（不静默按 S3 起）。
#[test]
fn backend_of_maps_cloud_budget_and_rejects_other_providers() {
  let option = KvOption {
    cloud_store_path: "bucket/prefix".to_owned(),
    local_space_limit: 12345,
    allow_reuse_local_caches: true,
    ..KvOption::default()
  };
  match backend_of(Some(&option), Path::new("fkv-root")).unwrap() {
    Backend::Cloud(cloud) => {
      assert_eq!(cloud.bucket, "bucket");
      assert_eq!(cloud.prefix, "prefix");
      assert_eq!(cloud.cache_group.limit(), 12345);
      assert!(
        cloud.allow_reuse_local_caches,
        "KvOption::allow_reuse_local_caches 必须投影进 CloudBackend"
      );
    }
    other => panic!("expected cloud backend, got {other:?}"),
  }

  let bad = KvOption {
    cloud_store_path: "bucket/prefix".to_owned(),
    cloud_provider: "gcs".to_owned(),
    ..KvOption::default()
  };
  assert_eq!(
    backend_of(Some(&bad), Path::new("fkv-root")).unwrap_err(),
    KvError::InvalidArgs,
    "非 aws/s3 provider 必须拒绝"
  );
}

/// 归档 crond 的 Cloud 目标：运行时侧 `CloudStore` 能在**非 compio** 线程构造
/// （它只做远端列举、不碰本地 cache），且与 shard 共用同一个预算组（12d）。
#[test]
fn cloud_store_builds_on_plain_thread_and_shares_budget_group() {
  let root = temp_root("cloud_store");
  let cfg = CloudStoreConfig {
    root: root.path().to_path_buf(),
    page_size: 512,
    pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    segment_size: 4096,
    segments_per_file_shift: 2,
    max_write_batch_pages: 64,
    max_inflight_write: 4096,
    fd_limit: 8,
    allow_reuse_local_caches: true,
  };
  let cloud = CloudBackend {
    bucket: "bucket".to_owned(),
    prefix: "prefix".to_owned(),
    endpoint: "http://127.0.0.1:1".to_owned(),
    region: "us-east-1".to_owned(),
    access_key: "k".to_owned(),
    secret_key: "s".to_owned(),
    allow_http: true,
    cache_group: Arc::new(CacheGroup::new(777)),
    allow_reuse_local_caches: true,
    max_cloud_concurrency: 32,
    s3_instance_num: 1,
    verify_ssl: true,
  };
  let store = open_runtime_cloud_store(&cfg, &cloud).unwrap();
  assert_eq!(store.cache_budget(), 777);
  assert!(
    store.reuse_local_caches(),
    "allow_reuse_local_caches 必须从装配参数透传到 CloudStore"
  );
  // 运行时侧实例必须与 shard 共用同一个预算组，不能各自一份上限。
  assert!(
    Arc::ptr_eq(store.cache_group(), &cloud.cache_group),
    "运行时侧 CloudStore 未共享 shard 的 CacheGroup"
  );
  // 装配自检：运行时侧（预热 / 全局列举 / 归档）不是分区写实例，必须只读，
  // 只有 shard 侧写实例允许启用表级 segment scheme。
  assert!(
    store.read_only(),
    "运行时侧 CloudStore 必须置 read_only(true)"
  );
}

/// 全局 fanout：按 `shard_index` 覆盖全部已 attach 分区、按
/// `max_global_request_batch` 分批（批大小 1 也全投），未 attach shard 的
/// `NotRunning` 在回执里可见而不是静默吞掉。
#[test]
fn global_drop_table_fans_out_and_reports_not_running() {
  pollster::block_on(async {
    let root = temp_root("drop");
    let option = KvOption {
      max_global_request_batch: 1,
      ..KvOption::default()
    };
    let runtime =
      Runtime::start_with_option(config(root.path().to_path_buf(), 2), &option).unwrap();
    // 2 shard：partition 1 / 3 → shard1，partition 2 → shard0。
    for part in [1u32, 2, 3] {
      fs::create_dir_all(root.join(format!("demo.{part}"))).unwrap();
    }
    // 另一张表的分区不能被 `global_drop_table("demo")` 波及。
    fs::create_dir_all(root.join("other.1")).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();

    let reply = runtime.global_drop_table("demo").await.unwrap();
    assert_eq!(reply.total(), 3, "{reply:?}");
    assert_eq!(reply.ok_num, 1, "{reply:?}");
    assert_eq!(reply.failed_li.len(), 2, "{reply:?}");
    for (tbl, err) in &reply.failed_li {
      assert_eq!(tbl.tbl_name.as_ref(), "demo");
      assert_eq!(tbl.shard_index(2), 1, "{tbl}");
      assert_eq!(*err, KvError::NotRunning);
    }

    // detach 后连 shard0 也回 NotRunning：fanout 仍然全投（不提前退出）。
    runtime.detach_shard(0).await.unwrap();
    // 第一次 drop 可能清掉 shard0 上的空分区目录，重新造出来再验回执。
    for part in [1u32, 2, 3] {
      fs::create_dir_all(root.join(format!("demo.{part}"))).unwrap();
    }
    let reply = runtime.global_drop_table("demo").await.unwrap();
    assert_eq!(reply.total(), 3, "{reply:?}");
    assert_eq!(reply.ok_num, 0, "{reply:?}");

    // 按表名过滤：另一张表只剩它自己的 1 个分区。
    let reply = runtime.global_drop_table("other").await.unwrap();
    assert_eq!(reply.total(), 1, "{reply:?}");
    assert_eq!(reply.ok_num, 0, "{reply:?}");
    assert_eq!(reply.failed_li[0].0.tbl_name.as_ref(), "other");
    assert_eq!(
      runtime.global_drop_table("").await.unwrap_err(),
      KvError::InvalidArgs
    );

    runtime.shutdown().await.unwrap();
  });
}

/// 全局列举归档 tag：直扫盘面（**未 attach 也要列出来**）、跳过无 tag 的 manifest
/// 与非 manifest 文件、按 `(tag, term)` 升序、全等 `(tag, term)` 跨分区去重
/// （C++ `sort` + `unique`），不同 term 的同名 tag 各占一条。
#[test]
fn global_list_archive_tags_scans_disk_and_sorts() {
  pollster::block_on(async {
    let root = temp_root("list_archive_tag_li");
    let runtime =
      Runtime::start_with_option(config(root.path().to_path_buf(), 2), &KvOption::default())
        .unwrap();
    for part in [1u32, 2] {
      fs::create_dir_all(root.join(format!("demo.{part}"))).unwrap();
    }
    let write = |part: u32, name: &str| {
      fs::write(root.join(format!("demo.{part}")).join(name), b"x").unwrap();
    };
    write(1, &fkv_types::archive_file_name("main", 2, "001"));
    write(1, &fkv_types::archive_file_name("branchA", 1, "001"));
    write(1, &fkv_types::archive_file_name("main", 2, "002"));
    write(2, &fkv_types::archive_file_name("main", 3, "0012"));
    // 跨分区全等 `(tag, term)`：fanout 归档的正常产物，列举只回一条。
    write(2, &fkv_types::archive_file_name("main", 2, "001"));
    // 负例：无 tag 的 manifest、current_term 文件、子目录、非法分区名。
    write(1, &fkv_types::manifest_file_name("main", 9));
    write(1, &fkv_types::current_term_file_name("main", 0));
    fs::create_dir_all(root.join("demo.1").join("nested")).unwrap();
    fs::create_dir_all(root.join("not_a_partition")).unwrap();

    // 排序键 = (tag, term)："0012" < "002"，同名 tag 按 term 升序。
    let entry_li = runtime.global_list_archive_tag_li("").await.unwrap();
    assert_eq!(
      entry_li,
      vec![
        ArchiveEntry {
          tag: "001".to_owned(),
          term: 1
        },
        ArchiveEntry {
          tag: "001".to_owned(),
          term: 2
        },
        ArchiveEntry {
          tag: "0012".to_owned(),
          term: 3
        },
        ArchiveEntry {
          tag: "002".to_owned(),
          term: 2
        },
      ],
      "{entry_li:?}"
    );

    // 前缀过滤（不要求整段相等，对齐 C++ rfind(prefix, 0)）。
    assert_eq!(
      runtime
        .global_list_archive_tag_li("00")
        .await
        .unwrap()
        .len(),
      4
    );
    assert_eq!(
      runtime.global_list_archive_tag_li("001").await.unwrap(),
      vec![
        ArchiveEntry {
          tag: "001".to_owned(),
          term: 1
        },
        ArchiveEntry {
          tag: "001".to_owned(),
          term: 2
        },
        ArchiveEntry {
          tag: "0012".to_owned(),
          term: 3
        },
      ]
    );
    assert!(
      runtime
        .global_list_archive_tag_li("zz")
        .await
        .unwrap()
        .is_empty()
    );

    runtime.shutdown().await.unwrap();
  });
}
