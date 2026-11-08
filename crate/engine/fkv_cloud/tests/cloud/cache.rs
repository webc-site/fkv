//! 缓存覆盖层、预算与对象 key 集成测试。

use super::*;

#[test]
fn cloud_overlay_cache_miss_and_reopen() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    let key_li = remote.list("fkv/demo.1/").await.unwrap();
    assert!(
      key_li
        .iter()
        .any(|meta| meta.key.starts_with("fkv/demo.1/data_"))
    );
    assert!(
      key_li
        .iter()
        .all(|meta| !meta.key.starts_with("fkv/demo.1/page/"))
    );
    drop(store);

    let mut cached = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    cached.reopen(&tbl).await.unwrap();
    assert_eq!(cached.get(&tbl, b"b").await.unwrap(), Some(b"B".to_vec()));
    // 缓存未命中时按整文件下载：index + data 至少两个 page 同时驻留。
    assert!(cached.page_io().local().page_count() >= 2);
    assert_eq!(cached.scan(&tbl, b"", b"", 0).await.unwrap().len(), 2);
  });
}

#[test]
fn whole_file_upload_is_deferred_until_sync() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    );
    for file_page_id in 0..3u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());
    cloud.sync_data(&tbl).await.unwrap();
    let meta_li = remote.list("fkv/demo.1/").await.unwrap();
    assert_eq!(meta_li.len(), 1);
    assert_eq!(meta_li[0].key, "fkv/demo.1/data_0_main_0");
    assert_eq!(meta_li[0].size, 3 * 512);
    assert!(remote.list("fkv/demo.1/page/").await.unwrap().is_empty());
  });
}

#[test]
fn cache_budget_evicts_and_redownloads_whole_files() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    // 4 pages / file = 2048 B；预算 3072 B，只容得下一个整文件。
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(remote.list("fkv/demo.1/").await.unwrap().len(), 2);
    assert_eq!(cloud.resident_file_count(), 1);
    assert_eq!(cloud.resident_bytes(), 2048);
    // 最久未使用的 file 0 被淘汰。
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_none());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_some());

    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    // 重新下载 file 0 后预算仍然生效：file 1 被淘汰。
    assert_eq!(cloud.resident_file_count(), 1);
    assert_eq!(cloud.resident_bytes(), 2048);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_none());
  });
}

/// 12d：两个实例共享一个 `CacheGroup` 时，整组驻留与本地实际占用都不超过
/// **单个** `local_space_limit`；对照组（各持一份预算）同样写入会翻倍，
/// 证明本测试能区分「共享记账」与「平行预算」。
#[test]
fn cache_group_bounds_parallel_instances_to_single_budget() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let group = Arc::new(CacheGroup::new(3072));
    // shard 侧与运行时侧预热：同一 root / 同一批文件，共享一份预算。
    let shard = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_group(group.clone());
    let prewarm = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_group(group.clone());

    // shard 写 2 个整文件 = 4096B > 3072B：flush 后淘汰最旧的 file 0。
    fill_file_li(&shard, &tbl, 2).await;
    assert_eq!(shard.resident_bytes(), 2048);
    assert_eq!(group.resident_bytes(), 2048);
    // 预热实例看到的余量必须是整组余量，不是自己那份 3072。
    assert_eq!(prewarm.cache_remained(), 1024);

    // 预热再把 file 0 拉进来：整组已装不下两份 2048B，实例只能摘自己的新条目。
    let mut page = vec![0u8; 512];
    prewarm.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    prewarm.drain_evictions().await.unwrap();
    shard.drain_evictions().await.unwrap();
    assert_eq!(prewarm.resident_bytes(), 0);
    assert_eq!(group.resident_bytes(), 2048);
    assert!(group.resident_bytes() <= 3072, "{group:?}");
    // 实际落盘字节（两个本地后端之和）同样不超过单个预算。
    let disk = local_bytes(&shard, &tbl).await + local_bytes(&prewarm, &tbl).await;
    assert_eq!(disk, 2048);

    // 只列举（归档 crond / global_* 的运行时侧实例）不写 cache、不占预算。
    let partition_page = prewarm
      .remote_partition_page(None, None, 100)
      .await
      .unwrap();
    assert_eq!(partition_page.table_li, vec![tbl.clone()]);
    assert_eq!(group.resident_bytes(), 2048);

    // 对照组：不共享预算时两个实例各容 2048B，合计 4096B 超过单个上界。
    let first = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    let second = CloudStore::new(MemStore::with_pages_per_file(512, 2), remote, "fkv", 2)
      .with_cache_budget(3072);
    fill_file_li(&first, &tbl, 2).await;
    fill_file_li(&second, &tbl, 2).await;
    let unshared = local_bytes(&first, &tbl).await + local_bytes(&second, &tbl).await;
    assert_eq!(unshared, 4096, "对照组应复现平行预算导致的 2× 上界");
  });
}

#[test]
fn write_to_evicted_file_restores_before_modifying() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072);
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    // file 0 已被淘汰，再写其中一页必须先恢复整文件。
    let page = vec![7u8; 512];
    cloud.write_page(&tbl, 1, &page).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let Some(bytes) = remote.get("fkv/demo.1/data_0_main_0").await.unwrap() else {
      panic!("file 0 must be uploaded");
    };
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 7);
    assert_eq!(bytes[2 * 512], 3);
  });
}

#[test]
fn drop_table_removes_local_and_remote_objects() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut store = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    store
      .put(
        &tbl,
        &[
          BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
          BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
        ],
      )
      .await
      .unwrap();
    assert!(!remote.list("fkv/demo.1/").await.unwrap().is_empty());

    store.drop_table(&tbl).await.unwrap();
    // Drop 只删活跃 manifest；data/segment 对象由 FileGc 按 retained set 回收
    // （对齐 C++ `DropManifest` + `TryAddFileGc`）。
    assert!(
      !remote
        .list("fkv/demo.1/")
        .await
        .unwrap()
        .iter()
        .any(|meta| meta.key.ends_with("manifest_main_0"))
    );
    store
      .file_gc(&tbl, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT)
      .await
      .unwrap();
    assert!(remote.list("fkv/demo.1/").await.unwrap().is_empty());
    assert_eq!(store.get(&tbl, b"a").await.unwrap(), None);

    store
      .put(&tbl, &[BatchEntry::upsert(b"x".to_vec(), b"X".to_vec(), 2)])
      .await
      .unwrap();
    assert_eq!(store.get(&tbl, b"x").await.unwrap(), Some(b"X".to_vec()));
    assert!(!remote.list("fkv/demo.1/").await.unwrap().is_empty());
  });
}

#[test]
fn cache_metrics_record_hit_miss_and_eviction() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let metrics = Arc::new(ShardMetric::new(0));
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(3072)
    .with_metrics(metrics.clone());
    for file_page_id in 0..8u64 {
      let mut page = vec![0u8; 512];
      page[0] = file_page_id as u8 + 1;
      cloud.write_page(&tbl, file_page_id, &page).await.unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert!(metrics.snapshot().cache_evict >= 1);

    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 1);
    cloud.read_page(&tbl, 0, &mut page).await.unwrap();
    let snapshot = metrics.snapshot();
    assert!(snapshot.cache_miss >= 1);
    assert!(snapshot.cache_hit >= 1);
    assert!(snapshot.cache_bytes > 0);
    assert_eq!(snapshot.cache_bytes, cloud.resident_bytes());
  });
}

/// 对齐 C++ `AsyncHttpManager::ComposeKey`：
/// `<prefix>/<table>.<partition>/<filename>`，filename 带 branch/term，
/// 没有 `data/`、`segment/`、`manifest/` 子目录；同表不同 branch 不冲突。
#[test]
fn cloud_object_keys_match_cpp_compose_key_layout() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    );
    let mut page = vec![0u8; 512];
    page[0] = 1;
    cloud.set_file_identity("feature", 7);
    cloud.write_page(&tbl, 0, &page).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let key_li: Vec<String> = remote
      .list("fkv/demo.1/")
      .await
      .unwrap()
      .into_iter()
      .map(|meta| meta.key)
      .collect();
    assert_eq!(key_li, vec!["fkv/demo.1/data_0_feature_7".to_owned()]);

    // 切回 main/0 写同一 file_id：对象 key 不同，互不覆盖。
    cloud.set_file_identity("main", 0);
    cloud.write_page(&tbl, 0, &page).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let key_li = remote.list("fkv/demo.1/").await.unwrap();
    assert!(
      key_li
        .iter()
        .any(|meta| meta.key == "fkv/demo.1/data_0_feature_7")
    );
    assert!(
      key_li
        .iter()
        .any(|meta| meta.key == "fkv/demo.1/data_0_main_0")
    );
    assert!(key_li.iter().all(|meta| !meta.key.contains("/data/")));
  });
}

#[test]
fn incremental_upload_only_sends_dirty_range_li() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    );
    let page = |tag: u8| {
      let mut page = vec![0u8; 512];
      page[0] = tag;
      page
    };
    cloud.write_page(&tbl, 0, &page(1)).await.unwrap();
    cloud.write_page(&tbl, 1, &page(2)).await.unwrap();
    cloud.write_page(&tbl, 2, &page(3)).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let baseline = remote.range_put_bytes();
    assert_eq!(baseline, 3 * 512);

    // 只重写中间页：上传字节应等于一页，而非整文件。
    cloud.write_page(&tbl, 1, &page(9)).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(remote.range_put_bytes() - baseline, 512);

    let bytes = remote
      .get("fkv/demo.1/data_0_main_0")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes.len(), 3 * 512);
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 9);
    assert_eq!(bytes[1024], 3);
  });
}
