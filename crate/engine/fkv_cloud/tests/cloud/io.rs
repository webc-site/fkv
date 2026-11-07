//! 页 / 文件读写、单飞与短文件重取测试。

use super::*;

/// io-29：分页列举按 token 衔接，跨页不重不漏；条目带对象大小。
#[test]
fn remote_file_id_page_paginates() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    for file_id in 0..5 {
      let name = fkv_types::data_file_name(file_id, "main", 0);
      remote
        .put(
          &format!("fkv/{tbl}/{name}"),
          Bytes::from_static(b"x"),
          None,
          false,
        )
        .await
        .unwrap();
    }
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut token: Option<String> = None;
    let mut file_id_li = Vec::new();
    loop {
      let page = cloud
        .remote_data_file_page(&tbl, token.as_deref(), 2)
        .await
        .unwrap();
      assert!(page.entry_li.len() <= 2);
      for entry in page.entry_li {
        assert_eq!(entry.size, 1);
        file_id_li.push(entry.file_id);
      }
      match page.next_token {
        Some(next) => token = Some(next),
        None => break,
      }
    }
    assert_eq!(file_id_li, (0..5).collect::<Vec<_>>());
  });
}

/// 远端分区列举：跨页用 `after`/`last` 去重，只产出 `{tbl}.{partition}`，
/// 无分区目录的 key（term 文件等）与 prefix 之外的 key 都跳过。
#[test]
fn remote_partition_page_dedups_across_pages() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let tbl_li = [
      TableIdent::new("demo", 1).unwrap(),
      TableIdent::new("demo", 3).unwrap(),
      TableIdent::new("other", 2).unwrap(),
    ];
    // 每个分区放 3 个对象：同一分区的 key 在字节序里连续，跨页只该产出一次。
    for tbl in &tbl_li {
      for file_id in 0..3 {
        let name = fkv_types::data_file_name(file_id, "main", 0);
        remote
          .put(
            &format!("fkv/{tbl}/{name}"),
            Bytes::from_static(b"x"),
            None,
            false,
          )
          .await
          .unwrap();
      }
    }
    // term 文件没有 `/`，不是分区目录。
    remote
      .put(
        "fkv/CURRENT_TERM_main_0",
        Bytes::from_static(b"0"),
        None,
        false,
      )
      .await
      .unwrap();
    // prefix 之外的 key 不参与列举。
    remote
      .put(
        "other-prefix/demo.9/data_0_main_0",
        Bytes::from_static(b"x"),
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
    let mut seen_li = Vec::new();
    let mut after: Option<TableIdent> = None;
    let mut token: Option<String> = None;
    loop {
      let page = cloud
        .remote_partition_page(after.as_ref(), token.as_deref(), 2)
        .await
        .unwrap();
      seen_li.extend(page.table_li);
      after = page.last;
      token = page.next_token;
      if token.is_none() {
        break;
      }
    }
    assert_eq!(seen_li, tbl_li);
    // max_keys=0（不限）一次取完应与翻页结果一致。
    let page = cloud.remote_partition_page(None, None, 0).await.unwrap();
    assert_eq!(page.table_li, tbl_li);

    // 换 prefix：原对象不可见。
    let other = CloudStore::new(
      MemStore::new(512),
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let page = other.remote_partition_page(None, None, 0).await.unwrap();
    assert!(page.table_li.is_empty());
  });
}

/// 本地 cache 只覆盖文件前缀（崩溃残留短文件）时，读越界页应按 miss 回源，
/// 而不是因为 `file_size().is_some()` 直接拒绝回源。
#[test]
fn read_page_refetches_after_local_short_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    {
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
      cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
      cloud.sync_data(&tbl).await.unwrap();
    }
    // 新进程本地只有第 0 页，第 1 页落在文件长度之外。
    let local = MemStore::new(512);
    local.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    let cloud = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
  });
}

/// 崩溃残留短文件在扩展写入前必须回源补齐：否则中段页会变成零洞。
#[test]
fn write_page_refetches_short_local_file_before_extending() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    {
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      for (page, tag) in [(0u64, 1u8), (1, 2), (2, 3)] {
        cloud.write_page(&tbl, page, &[tag; 512]).await.unwrap();
      }
      cloud.sync_data(&tbl).await.unwrap();
    }
    // 新进程本地只剩第 0 页（内容还是错的），写第 2 页前必须整文件回源。
    let local = MemStore::new(512);
    local.write_page(&tbl, 0, &[9u8; 512]).await.unwrap();
    let cloud = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 2, &[4u8; 512]).await.unwrap();
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
    cloud.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, [1u8; 512]);
  });
}

/// 换 term 后旧 term 的 dirty ranges 不能带到新 key：必须整对象上传，
/// 否则 `put_range` 会用零前缀把新对象首页冲成空洞。
#[test]
fn identity_change_discards_ranges_and_uploads_whole_object() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    // term 0 下再改一页，ranges 尚未上传。
    cloud.write_page(&tbl, 1, &[3u8; 512]).await.unwrap();
    cloud.set_file_identity("main", 1);
    cloud.sync_data(&tbl).await.unwrap();
    let bytes = remote
      .get("fkv/demo.1/data_0_main_1")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes.len(), 1024);
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 3);
  });
}

/// identity 变更后**首次** `mark_dirty_range` 也必须整对象上传：只记新区间
/// 会让新 term 对象前段成为零洞，旧脏页永久丢失。
#[test]
fn identity_change_then_dirty_range_uploads_whole_object() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    // term 0：只写第二页，保持脏且未上传。
    cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
    // 切到 term 1 后写第一页：这是 identity 变更后的首个脏区间。
    cloud.set_file_identity("main", 1);
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let bytes = remote
      .get("fkv/demo.1/data_0_main_1")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes.len(), 1024);
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[512], 2);
  });
}

/// 预热不得用远端旧对象覆盖本地脏页：本地命中直接返回，由 shard 自己的
/// flush 上传最新内容。
#[test]
fn prewarm_data_file_keeps_local_dirty_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    remote
      .put(
        "fkv/demo.1/data_0_main_0",
        Bytes::from(vec![9u8; 1024]),
        None,
        false,
      )
      .await
      .unwrap();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    assert!(cloud.prewarm_data_file(&tbl, 0).await.unwrap());
    cloud.sync_data(&tbl).await.unwrap();
    let bytes = remote
      .get("fkv/demo.1/data_0_main_0")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(bytes[0], 1);
  });
}

/// 子分支继承父分支数据：预热列举必须放行 manifest 区间引用的父 branch /
/// 旧 term 对象，同时仍拒绝无关 branch 与未被引用的旧 term。
#[test]
fn remote_file_page_includes_parent_branch_ranges() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    for name in [
      "data_0_main_1",
      "data_1_main_1",
      "data_2_feature_2",
      "data_3_feature_1",
      "data_4_other_1",
    ] {
      remote
        .put(
          &format!("fkv/demo.1/{name}"),
          Bytes::from_static(b"x"),
          None,
          false,
        )
        .await
        .unwrap();
    }
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    PageIo::set_file_identity(&cloud, "feature", 2);
    cloud.set_file_range(
      &tbl,
      &[
        BranchFileRange {
          branch_name: "main".into(),
          term: 1,
          max_file_id: 2,
          max_segment_file_id: 2,
        },
        BranchFileRange {
          branch_name: "feature".into(),
          term: 2,
          max_file_id: 4,
          max_segment_file_id: 4,
        },
      ],
    );
    let page = cloud.remote_data_file_page(&tbl, None, 1000).await.unwrap();
    let mut id_li: Vec<_> = page.entry_li.iter().map(|entry| entry.file_id).collect();
    id_li.sort_unstable();
    // 0/1 由父 branch 区间放行，2 走当前 term 快路径。
    assert_eq!(id_li, vec![0, 1, 2]);
  });
}

/// 冷启动 `restore_table_cache` 把崩溃残留的短文件按实际长度登记 resident 后，
/// 越界写仍必须回源补齐：只 `contains` 会放行半截文件，中段留零洞。
#[test]
fn write_page_refetches_after_restore_short_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    {
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      for (page, tag) in [(0u64, 1u8), (1, 2), (2, 3)] {
        cloud.write_page(&tbl, page, &[tag; 512]).await.unwrap();
      }
      cloud.sync_data(&tbl).await.unwrap();
    }
    // 新进程本地只剩第 0 页（内容还是错的），restore 按 512 登记 resident。
    let local = MemStore::new(512);
    local.write_page(&tbl, 0, &[9u8; 512]).await.unwrap();
    let cloud = CloudStore::new(
      local,
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.restore_table_cache(&tbl).await.unwrap();
    assert!(cloud.is_file_resident(&tbl, 0, false));
    cloud.write_page(&tbl, 2, &[4u8; 512]).await.unwrap();
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512], "restore 后的短文件必须先回源再越界写");
    cloud.read_page(&tbl, 0, &mut dst).await.unwrap();
    assert_eq!(dst, [1u8; 512], "回源必须覆盖崩溃残留的旧内容");
  });
}

/// 正常追加（写起点 == 当前 EOF）不得回源：判据是写起点而不是写末端，
/// 否则每次追加都会删本地副本 + 整文件 GET。
#[test]
fn append_after_sync_does_not_refetch_whole_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = CountingStore::new(false);
    let probe = remote.clone();
    let cloud = CloudStore::new(
      MemStore::with_pages_per_file(512, fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    cloud.write_page(&tbl, 0, &[1u8; 512]).await.unwrap();
    cloud.sync_data(&tbl).await.unwrap();
    let before = probe.get_count();
    // 页 1 的起点 == 页 0 的末端 == 本地/远端当前 EOF：纯追加。
    cloud.write_page(&tbl, 1, &[2u8; 512]).await.unwrap();
    assert_eq!(
      probe.get_count(),
      before,
      "追加写不得触发整文件回源（写起点判据）"
    );
    let mut dst = [0u8; 512];
    cloud.read_page(&tbl, 1, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
  });
}

/// 同一文件的 N 个并发冷读只允许一次远端 GET。
#[test]
fn single_flight_concurrent_cold_reads_fetch_once() {
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
    let probe = remote.clone();
    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let fut_li: Vec<_> = (0..8).map(|_| cloud.read_file(&tbl, 0)).collect();
    let result_li = join_all(fut_li).await;
    for result in result_li {
      assert_eq!(result.unwrap().unwrap(), vec![7u8; 512]);
    }
    assert_eq!(probe.get_count(), 1, "并发冷读必须单飞，只能回源一次");
  });
}

/// owner 被取消（future drop）后，等待者必须收到 `TryAgain` 而不是永久挂起。
#[test]
fn single_flight_waiter_returns_try_again_when_owner_dropped() {
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

    // owner 首次 poll 停在远端 get（yield_once），此时单飞登记已建立。
    let mut owner = Box::pin(cloud.read_file(&tbl, 0));
    assert!(owner.as_mut().poll(&mut cx).is_pending());
    // 后到者挂到同一 key 上等待。
    let mut waiter = Box::pin(cloud.read_file(&tbl, 0));
    assert!(waiter.as_mut().poll(&mut cx).is_pending());

    // 模拟 owner 被取消：DownloadGuard drop 必须唤醒等待者并下发 TryAgain。
    drop(owner);
    assert_eq!(waiter.await.unwrap_err(), KvError::TryAgain);
  });
}

/// `adopt_local_file` 的 segment 分支必须只做单文件完整性判定：
/// 逐文件列整表 segment 目录会让预热退化成 O(N²)。
#[test]
fn adopt_local_segment_file_does_not_list_table_dir() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let local = MemStore::new(512).with_segment_config(4096, 1);
    // slot_num = 2，尾槽 = file_id * 2 + 1；写满即完整。
    local.write_segment(&tbl, 1, &[7u8; 4096]).await.unwrap();
    let cloud = CloudStore::new(
      NoListStore { inner: local },
      MemObjectStore::new(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(4096, 1);
    assert!(
      cloud.adopt_local_file(&tbl, 0, true, 4096).await.unwrap(),
      "完整 segment 必须被登记"
    );
    assert!(cloud.is_file_resident(&tbl, 0, true));
    assert!(
      !cloud.adopt_local_file(&tbl, 1, true, 4096).await.unwrap(),
      "缺失的 segment 文件不能登记"
    );
  });
}

/// `ensure_resident` 的等待者路径必须强制回源：round 0 收到别人的 `Cached`
/// 不代表本地文件覆盖自己的写起点，按轮次关掉回源会留下零洞。
#[test]
fn ensure_resident_waiter_forces_own_refetch() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = GrowingStore::new();
    let probe = remote.clone();
    // pages_per_file_shift = 2：file 0 有 4 页，写第 3 页的起点是 1536。
    let cloud = CloudStore::new(MemStore::new(512), remote, "fkv", 2);
    let mut page = [0u8; 512];
    let mut reader = Box::pin(cloud.read_page(&tbl, 0, &mut page));
    let write_page = [3u8; 512];
    let mut writer = Box::pin(cloud.write_page(&tbl, 3, &write_page));
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    // reader 成为 owner 并停在远端 GET；writer 挂到同一 key 上。
    assert!(reader.as_mut().poll(&mut cx).is_pending());
    assert!(writer.as_mut().poll(&mut cx).is_pending());
    let (read_result, write_result) = futures::join!(reader, writer);
    read_result.unwrap();
    write_result.unwrap();

    assert_eq!(
      probe.get_count.load(Ordering::SeqCst),
      2,
      "等待者必须自己再回源一次"
    );
    // 回源后的完整文件覆盖 page 2；按短文件放行时这里会是零洞。
    let mut dst = [0u8; 512];
    cloud.local().read_page(&tbl, 2, &mut dst).await.unwrap();
    assert_eq!(dst, [2u8; 512]);
  });
}
