//! segment_tests

use core::{
  pin::pin,
  task::{Context, Poll},
};
use std::sync::Arc;

use bytes::Bytes as BytesBytes;
use fkv_error::KvError;
use fkv_io::{ManifestIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_types::TableIdent;
use futures::task::noop_waker;

use super::{CacheGroup, CloudStore, InflightDownload};
use crate::{
  segment::{SEGMENT_MARKER_MAGIC, SEGMENT_MARKER_PREFIX, segment_marker_name},
  test_support::{GetCountStore, ProbeStore, YieldWriteStore},
};

/// 第 2 项：scheme 回填必须跳过在飞下载 / 脏的 file id。
///
/// 回退即失败：不跳过时，回填按旧尾槽规则（定长段后端首写即预分配，
/// 尾槽可读零）给这些半装现场写「完整」标记。
#[test]
fn scheme_backfill_skips_inflight_and_dirty_segment() {
  pollster::block_on(async {
    let tbl = TableIdent::new("backfill_skip", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    // 旧目录：段 0 / 1 / 2 都按旧规则完整（首尾槽可读）。
    for file_id in 0..3u64 {
      cloud
        .local
        .write_segment(&tbl, file_id * 2, &[1u8; 4096])
        .await
        .unwrap();
      cloud
        .local
        .write_segment(&tbl, file_id * 2 + 1, &[0u8; 4096])
        .await
        .unwrap();
    }
    // 段 0 在飞（另一个 install 正在写）；段 1 有未上传脏数据。
    cloud.inflight_download.borrow_mut().insert(
      (tbl.clone(), true, 0),
      InflightDownload {
        generation: cloud.next_download_generation(),
        waiter_li: Vec::new(),
      },
    );
    cloud.upsert_segment_resident(&tbl, 1, 8192, true, None);

    cloud.enable_segment_marker(&tbl).await.unwrap();

    for file_id in [0, 1] {
      assert!(
        !cloud
          .segment_marker_present(&tbl, &segment_marker_name(file_id))
          .await
          .unwrap(),
        "在飞 / 脏段 {file_id} 不得被回填标记"
      );
    }
    assert!(
      cloud
        .segment_marker_present(&tbl, &segment_marker_name(2))
        .await
        .unwrap(),
      "无在飞 / 脏状态的旧段仍必须回填（升级兼容）"
    );
  });
}

/// 第 5 项：定长段后端首写即 `set_len` 整文件，半装文件尾槽也可读；
/// 只按尾槽判定会把半装文件永久当完整。显式完成标记缺失时必须判半装。
///
/// 回退即失败：旧实现只看尾槽，这里会返回 `true`。
#[test]
fn half_installed_segment_without_marker_is_incomplete() {
  pollster::block_on(async {
    let tbl = TableIdent::new("segmark", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    // 半装现场：本表已启用显式标记（install 先写 scheme），槽 0 已写，
    // 尾槽因预分配可读零，但单文件完成标记还没写。
    cloud
      .local
      .write_manifest_file(&tbl, SEGMENT_MARKER_PREFIX, SEGMENT_MARKER_MAGIC)
      .await
      .unwrap();
    cloud
      .local
      .write_segment(&tbl, 0, &[7u8; 4096])
      .await
      .unwrap();
    cloud
      .local
      .write_segment(&tbl, 1, &[0u8; 4096])
      .await
      .unwrap();
    assert!(
      !cloud.segment_file_complete(&tbl, 0).await.unwrap(),
      "缺完成标记必须判半装"
    );

    // 槽位全部落盘 + 标记写入后才是完整。
    let epoch = cloud.evict_epoch.get();
    cloud
      .install_segment_file(&tbl, 1, &[3u8; 8192], epoch)
      .await
      .unwrap();
    assert!(
      cloud.segment_file_complete(&tbl, 1).await.unwrap(),
      "写完完成标记后必须判完整"
    );
  });
}

/// 第 5 项兼容分支：旧目录没有 scheme 标记时继续按尾槽判定。
#[test]
fn legacy_segment_without_scheme_keeps_tail_check() {
  pollster::block_on(async {
    let tbl = TableIdent::new("seglegacy", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    cloud
      .local
      .write_segment(&tbl, 1, &[0u8; 4096])
      .await
      .unwrap();
    assert!(
      cloud.segment_file_complete(&tbl, 0).await.unwrap(),
      "旧目录（无 scheme 标记）必须保持尾槽判定，on-disk 兼容"
    );
    // 单文件标记名只在该表启用 scheme 后才被读取；旧目录下不参与判定。
    assert_eq!(segment_marker_name(0), "segcomplete_0");
  });
}

/// 第 4 项：首次建立 scheme 时给盘上已完整的旧段回填单文件标记，避免
/// `restore_table_cache` 把旧段当半装（不记账 → 预热误报 DiskFull）。
#[test]
fn enabling_scheme_backfills_existing_complete_segments() {
  pollster::block_on(async {
    let tbl = TableIdent::new("backfill", 1).unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    // 旧目录：段 0 完整（首尾槽都可读），无 scheme、无标记。
    cloud
      .local
      .write_segment(&tbl, 0, &[1u8; 4096])
      .await
      .unwrap();
    cloud
      .local
      .write_segment(&tbl, 1, &[0u8; 4096])
      .await
      .unwrap();
    assert!(
      cloud.segment_file_complete(&tbl, 0).await.unwrap(),
      "旧目录按旧规则判完整"
    );
    // 首次新格式安装触发 scheme；旧段应被回填标记。
    let epoch = cloud.evict_epoch.get();
    cloud
      .install_segment_file(&tbl, 1, &[3u8; 8192], epoch)
      .await
      .unwrap();
    assert!(
      cloud
        .segment_marker_present(&tbl, &segment_marker_name(0))
        .await
        .unwrap(),
      "旧段必须回填单文件标记"
    );
    assert!(
      cloud.segment_file_complete(&tbl, 0).await.unwrap(),
      "回填后旧段仍判完整"
    );
  });
}

/// 完成标记必须晚于段数据的 durability：否则崩溃后标记在、数据是零洞，
/// `restore_table_cache` 会把它登记成 clean 并读零上传。
#[test]
fn install_segment_syncs_slots_before_marker() {
  pollster::block_on(async {
    let tbl = TableIdent::new("syncorder", 1).unwrap();
    let probe = ProbeStore::new(4096, 1);
    let cloud = CloudStore::new(
      probe.clone(),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    let epoch = cloud.evict_epoch.get();
    cloud
      .install_segment_file(&tbl, 0, &[5u8; 8192], epoch)
      .await
      .unwrap();

    let log_li = probe.log_li();
    let sync_pos = log_li
      .iter()
      .position(|entry| entry == &format!("sync:{tbl}"))
      .expect("install 必须先 sync 段数据");
    let marker_pos = log_li
      .iter()
      .position(|entry| entry == "marker:segcomplete_0")
      .expect("install 必须写单文件完成标记");
    assert!(
      sync_pos < marker_pos,
      "完成标记不得早于数据落盘: {log_li:?}"
    );

    probe.crash();
    let mut dst = vec![0u8; 4096];
    cloud.read_segment(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, vec![5u8; 4096], "标记之前落盘的段数据必须扛住崩溃");
    assert!(cloud.segment_file_complete(&tbl, 0).await.unwrap());
  });
}

/// 回填必须跳过**同组另一实例**在飞 / 脏的段文件：运行时预热与 shard 是不同
/// `CloudStore` 实例，只看本实例状态会把对方半装文件盖成完整。
#[test]
fn scheme_backfill_skips_segment_busy_of_other_instance() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sharedbusy", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    let writer = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    // 写实例：段 0 在飞（另一任务 install 中），段 1 有未上传脏数据。
    group.set_segment_inflight(group.alloc_instance_id(), &tbl, 0, true);
    writer.upsert_segment_resident(&tbl, 1, 8192, true, None);

    let prewarm = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group);
    // 旧目录：段 0 / 1 / 2 都按旧尾槽规则完整。
    for file_id in 0..3u64 {
      prewarm
        .local
        .write_segment(&tbl, file_id * 2, &[1u8; 4096])
        .await
        .unwrap();
      prewarm
        .local
        .write_segment(&tbl, file_id * 2 + 1, &[0u8; 4096])
        .await
        .unwrap();
    }
    prewarm.enable_segment_marker(&tbl).await.unwrap();

    for file_id in [0, 1] {
      assert!(
        !prewarm
          .segment_marker_present(&tbl, &segment_marker_name(file_id))
          .await
          .unwrap(),
        "另一实例在飞 / 脏的段 {file_id} 不得被回填标记"
      );
    }
    assert!(
      prewarm
        .segment_marker_present(&tbl, &segment_marker_name(2))
        .await
        .unwrap(),
      "无跨实例 busy 状态的旧段仍必须回填"
    );
  });
}

/// R12-1：预热直连 `install_segment_file` 也必须在组内登记在飞。
///
/// 回退即失败：登记只留在 `single_flight` 里时预热不登记，同组回填会在
/// 段文件写到一半时把它当旧完整文件盖标记。
#[test]
fn prewarm_install_registers_group_inflight() {
  pollster::block_on(async {
    let tbl = TableIdent::new("prewarm_inflight", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    let remote = MemObjectStore::new();
    let prewarm = CloudStore::new(
      YieldWriteStore::new_segment_write_yield(),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    let other = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    let key = prewarm.segment_key(&tbl, 0);
    remote
      .put(&key, BytesBytes::from(vec![7u8; 8192]), None, false)
      .await
      .unwrap();
    assert!(remote.get(&key).await.unwrap().is_some(), "远端对象应可见");

    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut prewarm_fut = pin!(prewarm.prewarm_segment_file(&tbl, 0));
    // 远端 `get` 自身可能先挂起一次；推进到 install 入口登记后停下。
    for _ in 0..8 {
      if group.segment_busy(&tbl, 0) {
        break;
      }
      assert!(
        prewarm_fut.as_mut().poll(&mut cx).is_pending(),
        "应停在安装的段写入"
      );
    }
    assert!(group.segment_busy(&tbl, 0), "预热安装必须在组内登记在飞");
    assert!(
      other.segment_backfill_skip(&tbl, 0),
      "同组回填必须跳过预热正在写的段"
    );
    match prewarm_fut.as_mut().poll(&mut cx) {
      Poll::Ready(Ok(found)) => assert!(found),
      other => panic!("预热安装应完成，实际 {other:?}"),
    }
    assert!(!group.segment_busy(&tbl, 0), "安装结束后必须注销登记");
  });
}

/// R12-2：旧规则判定期间同组实例开始安装，写标记前必须复核 busy。
///
/// 回退即失败：只在循环开头判一次时，判定期间开始的 install 会被盖成完整。
#[test]
fn backfill_rechecks_busy_after_legacy_complete() {
  pollster::block_on(async {
    let tbl = TableIdent::new("backfill_recheck", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    let cloud = CloudStore::new(
      YieldWriteStore::new_segment_read_yield(),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    cloud
      .local
      .write_segment(&tbl, 0, &[1u8; 4096])
      .await
      .unwrap();
    cloud
      .local
      .write_segment(&tbl, 1, &[0u8; 4096])
      .await
      .unwrap();

    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut enable = pin!(cloud.enable_segment_marker(&tbl));
    assert!(
      enable.as_mut().poll(&mut cx).is_pending(),
      "应停在旧规则完整性判定的段读取"
    );
    // 判定期间另一实例开始安装同一段文件。
    group.set_segment_inflight(group.alloc_instance_id(), &tbl, 0, true);
    match enable.as_mut().poll(&mut cx) {
      Poll::Ready(Ok(())) => {}
      other => panic!("回填应完成，实际 {other:?}"),
    }
    assert!(
      !cloud
        .segment_marker_present(&tbl, &segment_marker_name(0))
        .await
        .unwrap(),
      "判定期间开始的安装必须让回填跳过该段"
    );
  });
}

/// R13-1：回填权被同组实例持有时，install 必须在删旧标记 / 写槽位之前退出。
///
/// 回退即失败：`enable_segment_marker` 返回 `Ok` 时安装会先删单文件标记再写
/// 半装槽位，此刻盘上没有 scheme，重启后按旧尾槽规则被误判完整。
#[test]
fn install_aborts_when_backfill_held_by_other_instance() {
  pollster::block_on(async {
    let tbl = TableIdent::new("enable_held", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    // 旧格式完整段 + 上一轮留下的单文件标记，盘上尚无 scheme。
    cloud
      .local
      .write_segment(&tbl, 0, &[1u8; 4096])
      .await
      .unwrap();
    cloud
      .local
      .write_segment(&tbl, 1, &[0u8; 4096])
      .await
      .unwrap();
    cloud.write_segment_marker(&tbl, 0).await.unwrap();

    let held = group.try_begin_enable(&tbl).expect("另一实例持有回填权");
    let epoch = cloud.evict_epoch.get();
    assert_eq!(
      cloud
        .install_segment_file(&tbl, 0, &[2u8; 8192], epoch)
        .await,
      Err(KvError::TryAgain),
      "回填权被持有时安装必须让路"
    );
    assert!(
      !cloud.segment_scheme_enabled(&tbl).await.unwrap(),
      "安装让路时不得落 scheme"
    );
    assert!(
      cloud
        .segment_marker_present(&tbl, &segment_marker_name(0))
        .await
        .unwrap(),
      "安装让路时不得先删旧完成标记"
    );
    drop(held);
  });
}

/// R13-2：`write_segment` 的组内在飞登记必须覆盖整个写（含本地写 await）。
///
/// 回退即失败：只在写完后标脏时，写 await 期间 `segment_busy` 为假，同组
/// 回填会按旧尾槽规则给半装文件盖「完整」标记。
#[test]
fn segment_write_registers_group_inflight_before_local_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("write_inflight", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    let cloud = CloudStore::new(
      YieldWriteStore::new_segment_write_yield(),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());

    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut write = pin!(cloud.write_segment(&tbl, 0, &[1u8; 4096]));
    for _ in 0..8 {
      if group.segment_busy(&tbl, 0) {
        break;
      }
      assert!(
        write.as_mut().poll(&mut cx).is_pending(),
        "应停在本地段写入"
      );
    }
    assert!(
      group.segment_busy(&tbl, 0),
      "本地写 await 期间必须在组内登记在飞"
    );
    assert!(
      cloud.segment_backfill_skip(&tbl, 0),
      "同组回填必须跳过正在写的段"
    );
    match write.as_mut().poll(&mut cx) {
      Poll::Ready(Ok(())) => {}
      other => panic!("段写应完成，实际 {other:?}"),
    }
    // 写完后条目转为脏 owner（flush 前本就 busy），只看在飞计数是否归零。
    let inflight_num = group
      .busy_guard()
      .get(&tbl)
      .and_then(|file_map| file_map.get(&0))
      .map_or(0, |entry| entry.inflight_num());
    assert_eq!(inflight_num, 0, "写结束后必须注销在飞登记");
  });
}

/// R14-3：同组另一实例持脏 / 在写同一段时，预热安装必须让路。
///
/// 回退即失败：不查组内 busy 时预热会用远端旧值覆盖 shard 正在写的脏段，
/// shard 随后 flush 会把旧值上传成权威（静默丢写）。
#[test]
fn prewarm_install_yields_to_foreign_dirty_segment() {
  pollster::block_on(async {
    let tbl = TableIdent::new("install_yield", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    // 共享本地 root：两个实例看同一批段文件。
    let probe = ProbeStore::new(4096, 1);
    let remote = MemObjectStore::new();
    let writer = CloudStore::new(
      probe.clone(),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    let prewarm = CloudStore::new(
      probe.clone(),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    // shard 已写段 3 的首槽（脏，未上传）；远端还有同一段文件的旧副本。
    writer
      .local
      .write_segment(&tbl, 6, &[1u8; 4096])
      .await
      .unwrap();
    writer.upsert_segment_resident(&tbl, 3, 8192, true, None);
    remote
      .put(
        &prewarm.segment_key(&tbl, 3),
        BytesBytes::from(vec![9u8; 8192]),
        None,
        false,
      )
      .await
      .unwrap();

    assert_eq!(
      prewarm.prewarm_segment_file(&tbl, 3).await,
      Err(KvError::TryAgain),
      "同组脏段必须让路"
    );
    let mut dst = vec![0u8; 4096];
    probe.read_segment(&tbl, 6, &mut dst).await.unwrap();
    assert_eq!(dst, vec![1u8; 4096], "shard 的写不得被远端旧值覆盖");

    // 无人在写的段照常安装，证明让路只针对冲突文件。
    remote
      .put(
        &prewarm.segment_key(&tbl, 4),
        BytesBytes::from(vec![8u8; 8192]),
        None,
        false,
      )
      .await
      .unwrap();
    assert!(prewarm.prewarm_segment_file(&tbl, 4).await.unwrap());
    probe.read_segment(&tbl, 8, &mut dst).await.unwrap();
    assert_eq!(dst, vec![8u8; 4096], "无冲突时以远端为准");
  });
}

/// R14-5：只读实例安装段文件，但不启用表级 scheme（不参与回填）。
///
/// 回退即失败：只读实例回填会按旧尾槽规则给**另一进程**正在写的半装文件
/// 盖「完整」标记，重启后读零并把零上传成权威。
#[test]
fn read_only_install_skips_scheme_backfill() {
  pollster::block_on(async {
    let tbl = TableIdent::new("read_only", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_read_only(true);
    assert!(cloud.read_only());
    remote
      .put(
        &cloud.segment_key(&tbl, 0),
        BytesBytes::from(vec![7u8; 8192]),
        None,
        false,
      )
      .await
      .unwrap();

    assert!(cloud.prewarm_segment_file(&tbl, 0).await.unwrap());
    assert!(
      cloud
        .segment_marker_present(&tbl, &segment_marker_name(0))
        .await
        .unwrap(),
      "单文件完成标记照常写"
    );
    assert!(
      !cloud.segment_scheme_enabled(&tbl).await.unwrap(),
      "只读实例不得落表级 scheme"
    );
    // 直接调用回填入口同样拒绝。
    cloud.enable_segment_marker(&tbl).await.unwrap();
    assert!(!cloud.segment_scheme_enabled(&tbl).await.unwrap());
  });
}

/// R14-2：回填权（scheme 启用）在 `remote.get` **之前**争取。
///
/// 回退即失败：先下载后抢权时，抢不到会白下载整个段对象再失败（压缩的
/// 8 路批量因此整批 abort）。
#[test]
fn install_claims_backfill_before_remote_get() {
  pollster::block_on(async {
    let tbl = TableIdent::new("claim_before_get", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    let remote = GetCountStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    remote
      .put(
        &cloud.segment_key(&tbl, 0),
        BytesBytes::from(vec![3u8; 8192]),
        None,
        false,
      )
      .await
      .unwrap();

    let held = group.try_begin_enable(&tbl).expect("另一实例持有回填权");
    assert_eq!(
      cloud.prewarm_segment_file(&tbl, 0).await,
      Err(KvError::TryAgain)
    );
    assert_eq!(remote.get_num.get(), 0, "抢不到回填权时不得先下载整对象");

    drop(held);
    assert!(cloud.prewarm_segment_file(&tbl, 0).await.unwrap());
    assert_eq!(remote.get_num.get(), 1, "拿到回填权后只下载一次");
  });
}
