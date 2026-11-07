//! 缓存淘汰、删除队列与容量核算测试。

use super::*;

/// 缓存按表嵌套后，detach 只删目标表的条目，`resident_bytes` 精确归零该表占用。
#[test]
fn evict_table_drops_only_that_table_cache() {
  pollster::block_on(async {
    let tbl_a = TableIdent::new("demo", 1).unwrap();
    let tbl_b = TableIdent::new("demo", 2).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(MemStore::with_pages_per_file(512, 2), remote, "fkv", 2);
    for (tbl, base) in [(&tbl_a, 0u8), (&tbl_b, 10u8)] {
      for file_page_id in 0..8u64 {
        let mut page = vec![0u8; 512];
        page[0] = base + file_page_id as u8 + 1;
        cloud.write_page(tbl, file_page_id, &page).await.unwrap();
      }
      cloud.sync_data(tbl).await.unwrap();
    }
    // shift=2 → 4 页/file（2048 B），每表 file 0/1 两个。
    assert_eq!(cloud.resident_file_count(), 4);
    assert_eq!(cloud.resident_bytes(), 8192);

    cloud.evict_table(&tbl_a);

    // 只掉了 tbl_a 的条目：计数与字节数各减一半；local 后端是否真删文件由后端自理
    // （`FileIo::evict_table` 默认 no-op）。
    assert_eq!(cloud.resident_file_count(), 2);
    assert_eq!(cloud.resident_bytes(), 4096);
    // tbl_b 不受影响：仍命中缓存，记账不变。
    let mut page = vec![0u8; 512];
    cloud.read_page(&tbl_b, 0, &mut page).await.unwrap();
    assert_eq!(page[0], 11);
    assert_eq!(cloud.resident_bytes(), 4096);
  });
}

/// 分区级 evict 后再次打开必须重新扫描本地文件并登记：只清记账不摘
/// `restored_table_set` 会让盘上文件永久脱钩预算 / LRU。
#[test]
fn evict_table_then_reopen_rescans_local_cache() {
  pollster::block_on(async {
    let tbl = TableIdent::new("rescan", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(MemStore::with_pages_per_file(512, 2), remote, "fkv", 2)
      .with_reuse_local_caches(true);
    // shift=2 → 4 页/file；8 个页 id 覆盖 file 0 / 1。
    for file_page_id in 0..8u64 {
      cloud
        .write_page(&tbl, file_page_id, &[file_page_id as u8 + 1; 512])
        .await
        .unwrap();
    }
    cloud.sync_data(&tbl).await.unwrap();
    assert_eq!(cloud.resident_file_count(), 2);

    cloud.evict_table(&tbl);
    assert_eq!(cloud.resident_file_count(), 0);
    assert_eq!(cloud.resident_bytes(), 0);

    // 重新打开：restore_table_cache 必须重扫盘上仍存在的 data 文件。
    cloud.read_manifest(&tbl).await.unwrap();
    assert_eq!(
      cloud.resident_file_count(),
      2,
      "evict 后 reopen 必须重新登记本地 cache"
    );
    assert_eq!(cloud.resident_bytes(), 4096);
  });
}

/// 全局侵入式 LRU：跨表按最近使用顺序淘汰，命中会把文件挪到最新端。
#[test]
fn cache_budget_evicts_globally_oldest_across_tables() {
  pollster::block_on(async {
    let tbl_a = TableIdent::new("a", 1).unwrap();
    let tbl_b = TableIdent::new("b", 1).unwrap();
    let tbl_c = TableIdent::new("c", 1).unwrap();
    let remote = MemObjectStore::new();
    // shift=2 → 4 页/file，写第 3 页即 2048 B；预算 2 个整文件。
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, 2),
      remote.clone(),
      "fkv",
      2,
    )
    .with_cache_budget(4096);
    let page = vec![7u8; 512];

    cloud.write_page(&tbl_a, 3, &page).await.unwrap();
    cloud.sync_data(&tbl_a).await.unwrap();
    cloud.write_page(&tbl_b, 3, &page).await.unwrap();
    cloud.sync_data(&tbl_b).await.unwrap();
    assert_eq!(cloud.resident_file_count(), 2);

    // 命中 a：a 变最新，LRU 旧端轮到 b。
    let mut dst = vec![0u8; 512];
    cloud.read_page(&tbl_a, 3, &mut dst).await.unwrap();
    assert_eq!(dst, page);

    // c 入 cache 触发淘汰：全局最久未用的 b 被踢，a 保留。
    cloud.write_page(&tbl_c, 3, &page).await.unwrap();
    assert!(cloud.local().file_size(&tbl_b, 0).await.unwrap().is_none());
    assert!(cloud.local().file_size(&tbl_a, 0).await.unwrap().is_some());
    assert!(cloud.local().file_size(&tbl_c, 0).await.unwrap().is_some());
    assert_eq!(cloud.resident_bytes(), 4096);
  });
}

/// 淘汰只摘链，删除按 `cache_budget / 8` 阈值批量执行；淘汰后又被写入的
/// 文件必须取消删除，不能误删刚回填的 cache。
#[test]
fn cache_evict_batches_deletes_and_keeps_refilled_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_remote_data(&remote, &tbl, 18).await;
    // 预算 8192 = 16 个 512B 文件；待删阈值 8192 / 8 = 1024 = 2 个文件。
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_cache_budget(8192);
    for file_id in 0..17u64 {
      assert!(cloud.read_file(&tbl, file_id).await.unwrap().is_some());
    }
    // 第 17 个文件只淘汰 file 0，待删 512B 未达阈值：本地删除延迟。
    assert_eq!(cloud.resident_bytes(), 8192);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());

    // 淘汰后重新写入 file 0：待删项在 drain 时必须跳过它，只删别的 victim。
    cloud.write_file(&tbl, 0, &[9u8; 512]).await.unwrap();
    assert_eq!(cloud.resident_bytes(), 8192);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_none());
  });
}

/// 待删队列未到阈值时，显式 `drain_evictions` 必须立即删除并清空队列。
#[test]
fn drain_evictions_flushes_pending_deletes() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_remote_data(&remote, &tbl, 17).await;
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_cache_budget(8192);
    for file_id in 0..17u64 {
      assert!(cloud.read_file(&tbl, file_id).await.unwrap().is_some());
    }
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());
    cloud.drain_evictions().await.unwrap();
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_none());
    assert_eq!(cloud.resident_bytes(), 8192);
  });
}

#[test]
fn drain_evictions_requeues_failed_delete() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    seed_remote_data(&remote, &tbl, 2).await;
    // 预算 512：第二个文件一入 cache 就淘汰 file 0，阈值 64 立即触发删除。
    let cloud = CloudStore::new(
      FlakyRemoveStore {
        inner: MemStore::new(512),
        fail_num: AtomicUsize::new(1),
      },
      remote,
      "fkv",
      18,
    )
    .with_cache_budget(512);
    assert!(cloud.read_file(&tbl, 0).await.unwrap().is_some());
    // 热路径删除失败不回传错误，victim 回队等待重试。
    assert!(cloud.read_file(&tbl, 1).await.unwrap().is_some());
    assert_eq!(cloud.resident_bytes(), 512);
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_some());

    cloud.drain_evictions().await.unwrap();
    assert!(cloud.local().file_size(&tbl, 0).await.unwrap().is_none());
    assert!(cloud.local().file_size(&tbl, 1).await.unwrap().is_some());
  });
}

/// 坏盘持续删除失败时，回队不能无界增长。
#[test]
fn evict_requeue_queue_is_bounded() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let file_num = EVICT_PENDING_FILE_MAX as u64 + 8;
    seed_remote_data(&remote, &tbl, file_num).await;
    let cloud = CloudStore::new(
      FlakyRemoveStore {
        inner: MemStore::new(512),
        fail_num: AtomicUsize::new(usize::MAX),
      },
      remote,
      "fkv",
      18,
    )
    .with_cache_budget(512);
    for file_id in 0..file_num {
      cloud.read_file(&tbl, file_id).await.unwrap();
    }
    assert!(cloud.pending_delete_count() <= EVICT_PENDING_FILE_MAX);
    assert!(cloud.pending_delete_count() > 0);
  });
}

/// 冷启动后必须按本地物理文件重建记账：否则旧文件不计预算、不参与淘汰，
/// 预热还会把已落盘文件当成 miss。
#[test]
fn restore_local_cache_accounting_after_restart() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    PageIo::set_file_identity(&cloud, "main", 1);
    cloud.write_page(&tbl, 0, &[7u8; 512]).await.unwrap();
    assert_eq!(cloud.resident_bytes(), 512);

    let (local, _) = cloud.into_parts();
    let restarted = CloudStore::new(
      local,
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    restarted.restore_table_cache(&tbl).await.unwrap();
    assert!(restarted.is_file_resident(&tbl, 0, false));
    assert_eq!(restarted.resident_bytes(), 512);

    // `allow_reuse_local_caches=false`：拒绝复用，清空本地 data / segment。
    let (local, _) = restarted.into_parts();
    let cleared = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_reuse_local_caches(false);
    cleared.restore_table_cache(&tbl).await.unwrap();
    assert_eq!(cleared.resident_bytes(), 0);
    assert!(
      cleared
        .local()
        .list_file_id_li(&tbl)
        .await
        .unwrap()
        .is_empty()
    );
  });
}

/// `cache_local_file` 必须把「写后文件不存在」当失败：按 `bytes.len()` 登记
/// resident 会让后续读写不再回源，直到进程重启。
#[test]
fn cache_local_file_rejects_missing_file_after_write() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let name = fkv_types::data_file_name(0, "main", 0);
    remote
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![7u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      NoSizeStore {
        inner: MemStore::new(512),
      },
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    assert_eq!(cloud.read_file(&tbl, 0).await.unwrap_err(), KvError::IoFail);
    assert_eq!(cloud.resident_file_count(), 0, "不能登记未落盘的文件");
  });
}

/// 驱逐表时必须摘掉该表的在飞下载：等待者收到 `TryAgain`，owner 完成后也
/// 不能把已 detach 的表重新落盘 / `upsert_resident`（reopen 会当权威）。
#[test]
fn evict_table_cancels_inflight_download_and_rejects_stale_install() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = CountingStore::new(true);
    let name = fkv_types::data_file_name(0, "main", 0);
    remote
      .inner
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![7u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut owner = Box::pin(cloud.read_file(&tbl, 0));
    assert!(owner.as_mut().poll(&mut cx).is_pending());
    let mut waiter = Box::pin(cloud.read_file(&tbl, 0));
    assert!(waiter.as_mut().poll(&mut cx).is_pending());

    cloud.evict_table(&tbl);
    assert_eq!(
      waiter.await.unwrap_err(),
      KvError::TryAgain,
      "驱逐后等待者不能被挂死"
    );
    assert_eq!(
      owner.await.unwrap_err(),
      KvError::TryAgain,
      "owner 的下载结果必须作废"
    );
    assert!(
      cloud.local().file_size(&tbl, 0).await.unwrap().is_none(),
      "已驱逐的表不得留下落盘文件"
    );
    assert_eq!(cloud.resident_file_count(), 0);
  });
}

/// 在远端造 `file_num` 个 512B（单页）data 对象。
async fn seed_remote_data(remote: &MemObjectStore, tbl: &TableIdent, file_num: u64) {
  for file_id in 0..file_num {
    let name = fkv_types::data_file_name(file_id, "main", 0);
    // 非零内容：MemStore 把全零块当空洞，全零文件不会落本地。
    remote
      .put(
        &format!("fkv/{tbl}/{name}"),
        Bytes::from(vec![(file_id % 255 + 1) as u8; 512]),
        None,
        false,
      )
      .await
      .unwrap();
  }
}
