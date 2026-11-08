use std::{
  fs::read_dir,
  path::Path,
  sync::Arc,
  thread,
  thread::sleep,
  time::{Duration, Instant},
};

use compio::runtime;
use fkv_btree::BatchEntry;
use fkv_config::KvOption;
use fkv_error::KvError;
use fkv_io_compio::CompioStore;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_store::Store;
use fkv_testkit::TempRoot;
use fkv_types::{TableIdent, parse_manifest_file_name};

fn archive_num(root: &Path) -> usize {
  archive_num_of(root, 1)
}

fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
  let deadline = Instant::now() + timeout;
  while Instant::now() < deadline {
    if predicate() {
      return true;
    }
    thread::sleep(Duration::from_millis(50));
  }
  predicate()
}

#[test]
fn archive_crond_archives_attached_shards_until_detach() {
  let root = TempRoot::new("archive_crond");
  pollster::block_on(async {
    let runtime = Runtime::start_with_option(
      RuntimeConfig {
        num_threads: 1,
        root: root.path().to_path_buf(),
        page_size: 512,
        restart_interval: 16,
        pages_per_file_shift: fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
        segment_size: 4096,
        segments_per_file_shift: 2,
        large_value_threshold: 1024,
        fixed_buffer_size: 0,
        queue_size: 64,
        auto_reopen_retry_times: 0,
        auto_reopen_pending_time_us: 0,
        archive_interval_secs: 1,
        archive_num_retained: 2,
      },
      &KvOption {
        // 自动归档只对 append 分区生效（C++ `data_append_mode` 门）。
        data_append_mode: true,
        ..KvOption::default()
      },
    )
    .unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();

    assert!(
      wait_until(|| archive_num(root.path()) >= 1, Duration::from_secs(6)),
      "first archive not created"
    );
    assert!(
      wait_until(|| archive_num(root.path()) >= 2, Duration::from_secs(5)),
      "second archive not created"
    );

    runtime.detach_shard(0).await.unwrap();
    thread::sleep(Duration::from_millis(500));
    let after_detach = archive_num(root.path());
    thread::sleep(Duration::from_millis(1300));
    assert_eq!(
      archive_num(root.path()),
      after_detach,
      "detached shard must not be archived"
    );
    runtime.shutdown().await.unwrap();
  });
}

fn config(root: &Path, num_threads: u16, archive_interval_secs: u64) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root.to_path_buf());
  config.num_threads = num_threads;
  config.archive_interval_secs = archive_interval_secs;
  config.archive_num_retained = 4;
  config
}

/// 线程 / 目录创建可能受同一 runner 上并行测试的资源压力影响；`IoFail` 时短暂
/// 重试，避免把资源瞬时耗尽误报成功能失败。
fn start(config: RuntimeConfig) -> Runtime {
  let option = KvOption {
    data_append_mode: true,
    ..KvOption::default()
  };
  for _ in 0..5 {
    match Runtime::start_with_option(config.clone(), &option) {
      Ok(runtime) => return runtime,
      Err(KvError::IoFail) => sleep(Duration::from_millis(20)),
      Err(err) => panic!("runtime start failed: {err:?}"),
    }
  }
  Runtime::start_with_option(config, &option).unwrap()
}

fn archive_num_of(root: &Path, partition_id: u32) -> usize {
  let Ok(entry_li) = read_dir(root.join(format!("demo.{partition_id}"))) else {
    return 0;
  };
  entry_li
    .flatten()
    .filter(|entry| {
      entry
        .file_name()
        .to_str()
        .is_some_and(|name| name.starts_with("manifest_main_1_"))
    })
    .count()
}

/// 锁定「全局 tick」：attach 后的首个归档落在下一个全局 tick，而不是
/// attach + interval（C++ `ArchiveCrond::AddShard` 的 per-shard 语义）。
///
/// 在 interval 后半段 attach 一个盘上已有分区的 shard：若按 per-shard 独立
/// 调度，首个归档要到 attach + interval；全局 tick 下它落在 attach 后的第一个
/// tick，延迟明显小于一个 interval。
#[test]
fn archive_crond_uses_global_tick() {
  const INTERVAL_SECS: u64 = 4;
  let root = TempRoot::new("archive_crond");
  // 预置分区 1（路由到 shard 1）的盘面数据，attach 前就已存在。
  let tbl = TableIdent::new("demo", 1).unwrap();
  let writer = runtime::Runtime::new().unwrap();
  writer.block_on(async {
    let mut store = Store::new(
      CompioStore::new(
        root.path().to_path_buf(),
        512,
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
  });

  pollster::block_on(async {
    let runtime = start(config(root.path(), 2, INTERVAL_SECS));
    // 跳过本轮 tick 的前半段：attach 到下一个全局 tick 的间隔明显小于 interval。
    thread::sleep(Duration::from_millis(INTERVAL_SECS * 1000 / 2));
    let attached_at = Instant::now();
    runtime.attach_shard(1, "main", 1).await.unwrap();
    assert!(
      wait_until(
        || archive_num_of(root.path(), 1) >= 1,
        Duration::from_secs(3)
      ),
      "全局 tick 下 attach 后的首个归档必须落在下一个 tick"
    );
    let first_archive_delay = attached_at.elapsed();
    assert!(
      first_archive_delay < Duration::from_secs(INTERVAL_SECS),
      "首个归档必须早于 attach + interval（per-shard 独立调度），实际 {first_archive_delay:?}"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// 长间隔下 shutdown 不能被 crond 的等待拖住。
#[test]
fn archive_crond_stops_promptly() {
  let root = TempRoot::new("archive_crond");
  pollster::block_on(async {
    let runtime = start(config(root.path(), 2, 3600));
    for shard_id in 0..2 {
      runtime.attach_shard(shard_id, "main", 1).await.unwrap();
    }
    let tbl = TableIdent::new("demo", 0).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    let started = Instant::now();
    runtime.shutdown().await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
  });
}

/// 多 shard 并发归档，同时多线程并发请求。
#[test]
fn archive_crond_multi_shard_with_concurrent_request() {
  const SHARD_NUM: u16 = 2;
  const WRITER_NUM: u32 = 4;
  const PUT_NUM: u64 = 64;
  let root = TempRoot::new("archive_crond");
  let runtime = Arc::new(start(config(root.path(), SHARD_NUM, 1)));
  pollster::block_on(async {
    for shard_id in 0..u32::from(SHARD_NUM) {
      runtime.attach_shard(shard_id, "main", 1).await.unwrap();
    }
  });
  let writer_li: Vec<_> = (0..WRITER_NUM)
    .map(|writer| {
      let runtime = runtime.clone();
      thread::spawn(move || {
        let tbl = TableIdent::new("demo", writer % u32::from(SHARD_NUM)).unwrap();
        pollster::block_on(async {
          for seq in 0..PUT_NUM {
            let key = format!("w{writer}_{seq:04}").into_bytes();
            runtime
              .put(&tbl, vec![BatchEntry::upsert(key.clone(), key.clone(), 1)])
              .await
              .unwrap();
            assert_eq!(runtime.get(&tbl, &key).await.unwrap(), Some(key));
          }
        });
      })
    })
    .collect();
  writer_li
    .into_iter()
    .for_each(|writer| writer.join().unwrap());
  assert!(
    wait_until(
      || (0..u32::from(SHARD_NUM)).all(|partition| archive_num_of(root.path(), partition) >= 1),
      Duration::from_secs(6),
    ),
    "every attached shard must be archived"
  );
  let Ok(runtime) = Arc::try_unwrap(runtime) else {
    panic!("runtime still shared");
  };
  pollster::block_on(runtime.shutdown()).unwrap();
}

/// 只读表：本进程从未写入、盘上已有数据的分区也要被定时归档
/// （对齐 C++ `HandleGlobalArchiveRequest` 每轮重新列举 store_path）。
#[test]
fn archive_crond_archives_disk_partitions_never_written_here() {
  let root = TempRoot::new("archive_crond");
  let tbl = TableIdent::new("demo", 1).unwrap();
  // 先落盘（模拟上一轮进程写入的数据）；本进程只 attach + 读，不写。
  let writer = runtime::Runtime::new().unwrap();
  writer.block_on(async {
    let mut store = Store::new(
      CompioStore::new(
        root.path().to_path_buf(),
        512,
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    store
      .put(&tbl, &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)])
      .await
      .unwrap();
  });
  assert_eq!(archive_num(root.path()), 0);

  pollster::block_on(async {
    let runtime = start(config(root.path(), 1, 1));
    runtime.attach_shard(0, "main", 1).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert!(
      wait_until(|| archive_num(root.path()) >= 1, Duration::from_secs(6)),
      "本进程未写过的只读表没有被归档"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// 空 tag：自动生成定宽微秒时间戳（对齐 C++ 空 tag 走时间戳；定宽保证
/// `prune_archive_li` 的名字排序 = 时间排序）。
#[test]
fn create_archive_empty_tag_generates_timestamp() {
  let root = TempRoot::new("archive_crond");
  pollster::block_on(async {
    let runtime = start(config(root.path(), 1, 0));
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    let reply = runtime.create_archive(&tbl, "", 4).await.unwrap();
    let (branch, _, tag) = parse_manifest_file_name(&reply.name).unwrap();
    assert_eq!(branch, "main");
    let tag = tag.expect("自动 tag 必须写进文件名");
    assert_eq!(tag.len(), 20, "tag 必须定宽 20 位：{tag}");
    assert!(
      tag.bytes().all(|byte| byte.is_ascii_digit()),
      "tag 必须是十进制时间戳：{tag}"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// `start_with_option`：crond 由 KvOption 的归档参数驱动（对齐 C++ 由 KvOption
/// 的 `archive_interval_secs` / `num_retained_archives` 决定）。
#[test]
fn archive_crond_follows_kv_options_archive_params() {
  let root = TempRoot::new("archive_crond");
  let option = KvOption {
    data_append_mode: true,
    archive_interval_secs: 1,
    num_retained_archives: 2,
    ..KvOption::default()
  };
  pollster::block_on(async {
    // RuntimeConfig 的归档间隔为 0：若仍以它为准，crond 根本不会启动。
    // `archive_num_retained` 保持 KvOption 默认值（100），避免与 option 显式冲突
    // （双显式且不同现在直接 InvalidArgs）。
    let mut config = config(root.path(), 1, 0);
    config.archive_num_retained = KvOption::default().num_retained_archives;
    let runtime = Runtime::start_with_option(config, &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert!(
      wait_until(|| archive_num(root.path()) >= 1, Duration::from_secs(6)),
      "KvOption 的归档参数没有驱动 crond"
    );
    runtime.shutdown().await.unwrap();
  });
}

/// `num_retained_archives = 0` 时不启动归档 crond（对齐 C++
/// `eloq_store.cpp:898` 的三项与）。`num_retained=0` 只关闭自动归档；
/// 显式 `create_archive` 仍按「不剪枝」执行（见 fkv_store 的保留策略测试）。
#[test]
fn archive_crond_disabled_when_num_retained_zero() {
  let root = TempRoot::new("archive_crond");
  let option = KvOption {
    data_append_mode: true,
    archive_interval_secs: 1,
    num_retained_archives: 0,
    ..KvOption::default()
  };
  pollster::block_on(async {
    // RuntimeConfig 的归档间隔为 0：crond 只能由 option 的 interval=1 驱动。
    // `archive_num_retained` 保持 KvOption 默认值（100），避免与 option 显式冲突
    // （双显式且不同现在直接 InvalidArgs）。
    let mut config = config(root.path(), 1, 0);
    config.archive_num_retained = KvOption::default().num_retained_archives;
    let runtime = Runtime::start_with_option(config, &option).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"k".to_vec(), b"v".to_vec(), 1)],
      )
      .await
      .unwrap();
    // 分区目录必须已存在，否则 archive_num=0 可能只是目录不存在而非未归档。
    assert!(root.path().join("demo.1").exists());
    // 等待 > 1 个 interval：若 crond 被错误启动，至少会落一份归档 manifest。
    thread::sleep(Duration::from_millis(2200));
    assert_eq!(
      archive_num(root.path()),
      0,
      "num_retained_archives=0 时不得自动归档（对齐 C++）"
    );
    runtime.shutdown().await.unwrap();
  });
}
