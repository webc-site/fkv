//! group_tests

use std::sync::Arc;

use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_object::MemObjectStore;
use fkv_types::{FileId, TableIdent};

use super::{CacheGroup, CloudStore};
use crate::{group::PublishGuard, test_support::ProbeStore};

/// R12-2：回填权在组上串行，另一实例正在回填时不得再落 scheme。
///
/// 回退即失败：实例私有登记时两个实例各回填一遍，会互相给在写文件盖标记。
#[test]
fn scheme_enabling_is_serialized_across_group_instances() {
  pollster::block_on(async {
    let tbl = TableIdent::new("group_enable", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    let mut store_li = (0..2)
      .map(|_| {
        CloudStore::new(
          MemStore::new(512).with_segment_config(4096, 1),
          MemObjectStore::new(),
          "fkv",
          fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
        )
        .with_segment_config(4096, 1)
        .with_cache_group(group.clone())
      })
      .collect::<Vec<_>>();
    let held = group.try_begin_enable(&tbl).expect("首个回填者取得回填权");
    let other = store_li.pop().unwrap();
    assert_eq!(
      other.enable_segment_marker(&tbl).await,
      Err(KvError::TryAgain),
      "抢不到回填权必须 TryAgain，不得报成功"
    );
    assert!(
      !other.segment_scheme_enabled(&tbl).await.unwrap(),
      "另一实例持有回填权时不得落 scheme"
    );
    drop(held);
    other.enable_segment_marker(&tbl).await.unwrap();
    assert!(
      other.segment_scheme_enabled(&tbl).await.unwrap(),
      "回填权释放后必须能落 scheme"
    );
  });
}

/// R12-3：共享组比实例长寿时，实例销毁必须清掉自己的脏 owner。
///
/// 回退即失败：无释放时 owner 永久残留，该段在组内永久 busy，回填永远跳过。
#[test]
fn dropping_store_releases_group_dirty_owner() {
  let tbl = TableIdent::new("owner_drop", 1).unwrap();
  let group = Arc::new(CacheGroup::new(0));
  {
    let store = CloudStore::new(
      MemStore::new(512).with_segment_config(4096, 1),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    store.upsert_segment_resident(&tbl, 1, 8192, true, None);
    assert!(group.segment_busy(&tbl, 1));
  }
  assert!(!group.segment_busy(&tbl, 1), "实例销毁必须释放组内 owner");
}

/// R12-4：本地表 / 条目缺失时 `mark_segment_clean` 也必须清 owner。
///
/// 回退即失败：提前 return 会跳过 owner 清理，文件在组内永久 busy。
#[test]
fn mark_segment_clean_clears_owner_without_local_entry() {
  let tbl = TableIdent::new("clean_missing", 1).unwrap();
  let cloud = CloudStore::new(
    MemStore::new(512).with_segment_config(4096, 1),
    MemObjectStore::new(),
    "fkv",
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 1);
  cloud.owner.set_segment_dirty(cloud.owner.id, &tbl, 7, true);
  assert!(cloud.owner.segment_busy(&tbl, 7));
  cloud.mark_segment_clean(&tbl, 7, 0);
  assert!(
    !cloud.owner.segment_busy(&tbl, 7),
    "本地缺条目也必须清 owner"
  );
}

/// R12-5：busy 查询按表作用域，同 file id 的另一张表不得串扰。
#[test]
fn segment_busy_is_scoped_by_table() {
  let tbl_a = TableIdent::new("busy_a", 1).unwrap();
  let tbl_b = TableIdent::new("busy_b", 1).unwrap();
  let group = CacheGroup::new(0);
  let owner = group.alloc_instance_id();
  group.set_segment_inflight(owner, &tbl_a, 0, true);
  assert!(group.segment_busy(&tbl_a, 0));
  assert!(!group.segment_busy(&tbl_b, 0), "另一张表必须不受影响");
  assert!(!group.segment_busy(&tbl_a, 1), "另一个文件必须不受影响");
  group.set_segment_inflight(owner, &tbl_a, 0, false);
  assert!(!group.segment_busy(&tbl_a, 0));
}

/// R13-3：发布登记必须跨实例可见，prewarm 不得删掉 shard 正在发布的首写快照。
///
/// 回退即失败：登记留在实例私有 `RefCell` 时，prewarm 看不到 shard 的首写
/// 窗口，`(Some, None)` 分支会把本地 manifest 删掉。
#[test]
fn publish_inflight_is_visible_across_group_instances() {
  pollster::block_on(async {
    let tbl = TableIdent::new("publish_shared", 1).unwrap();
    let group = Arc::new(CacheGroup::new(0));
    // 共享本地 root：同一个 `ProbeStore` 句柄。
    let probe = ProbeStore::new(4096, 1);
    let writer = CloudStore::new(
      probe.clone(),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());
    let prewarm = CloudStore::new(
      probe.clone(),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1)
    .with_cache_group(group.clone());

    writer.local.append_manifest(&tbl, b"rec").await.unwrap();
    let _publish = PublishGuard::new(&writer.owner.group, &tbl);
    // 远端还没有 manifest：shard 的 PUT 尚在发布中。
    assert!(
      !prewarm.prewarm_manifest(&tbl).await.unwrap(),
      "发布中不应宣称本地快照可用"
    );
    assert!(
      probe.read_manifest(&tbl).await.unwrap().is_some(),
      "另一实例的发布窗口内不得删除本地 manifest"
    );
  });
}

/// R13-4：淘汰 segment 条目必须清组内本 owner 的脏登记。
///
/// 回退即失败：owner 随条目摘链残留，该文件在组内永久 busy，scheme 回填
/// 永远跳过它。
#[test]
fn trim_cache_clears_group_dirty_owner_on_segment_victim() {
  let tbl = TableIdent::new("evict_owner", 1).unwrap();
  let cloud = CloudStore::new(
    MemStore::new(512).with_segment_config(4096, 1),
    MemObjectStore::new(),
    "fkv",
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 1)
  .with_cache_budget(1);
  cloud.upsert_segment_resident(&tbl, 7, 8192, false, None);
  cloud.owner.set_segment_dirty(cloud.owner.id, &tbl, 7, true);
  assert!(cloud.owner.segment_busy(&tbl, 7));

  assert_eq!(cloud.trim_cache(), 1, "超预算必须淘汰该干净段");
  assert!(
    !cloud.owner.segment_busy(&tbl, 7),
    "淘汰摘链后必须清组内 owner"
  );
  assert_eq!(cloud.resident_file_count(), 0);
}

/// R14-4：在飞登记按 owner 记深度：同实例嵌套登记不失衡，他人登记可见。
///
/// 回退即失败：全局计数（旧实现）下写路径 guard 与嵌套 install 计 2，
/// 内层注销就把外层仍在写的文件置空；`saturating_sub` 还会掩盖失衡。
#[test]
fn segment_inflight_is_tracked_per_owner_with_nesting() {
  let tbl = TableIdent::new("owner_depth", 1).unwrap();
  let group = CacheGroup::new(0);
  let owner_a = group.alloc_instance_id();
  let owner_b = group.alloc_instance_id();

  group.set_segment_inflight(owner_a, &tbl, 1, true);
  group.set_segment_inflight(owner_a, &tbl, 1, true);
  group.set_segment_inflight(owner_a, &tbl, 1, false);
  assert!(group.segment_busy(&tbl, 1), "嵌套登记不得被内层注销抹掉");
  assert!(group.segment_foreign_busy(owner_b, &tbl, 1));
  assert!(
    !group.segment_foreign_busy(owner_a, &tbl, 1),
    "自己的登记不算他人"
  );
  group.set_segment_inflight(owner_a, &tbl, 1, false);
  assert!(!group.segment_busy(&tbl, 1), "配对注销后必须归零");

  // 未配对的注销不得减别人的深度。
  group.set_segment_inflight(owner_b, &tbl, 2, true);
  group.set_segment_inflight(owner_a, &tbl, 2, false);
  assert!(group.segment_busy(&tbl, 2), "别人的在飞登记必须保留");
}

/// R14-6：表级清理一次加锁批量清脏登记，且只清本 owner。
#[test]
fn clear_segment_dirty_li_batches_owner_cleanup() {
  let tbl = TableIdent::new("batch_clear", 1).unwrap();
  let group = CacheGroup::new(0);
  let owner_a = group.alloc_instance_id();
  let owner_b = group.alloc_instance_id();
  let file_id_li: Vec<FileId> = (0..8).collect();
  for file_id in &file_id_li {
    group.set_segment_dirty(owner_a, &tbl, *file_id, true);
  }

  group.clear_segment_dirty_li(owner_a, &tbl, &file_id_li);
  for file_id in &file_id_li {
    assert!(
      !group.segment_busy(&tbl, *file_id),
      "段 {file_id} 的本 owner 脏登记必须清掉"
    );
  }
  // 别的 owner 的登记不属于这一批清理对象。
  group.set_segment_dirty(owner_b, &tbl, 3, true);
  group.set_segment_dirty(owner_b, &tbl, 5, true);
  group.clear_segment_dirty_li(owner_a, &tbl, &file_id_li);
  for file_id in [3, 5] {
    assert!(
      group.segment_foreign_busy(owner_a, &tbl, file_id),
      "批量清理不得动别的 owner"
    );
  }
}
