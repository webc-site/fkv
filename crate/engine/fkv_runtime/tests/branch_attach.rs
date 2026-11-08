use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[test]
fn attach_branch_opens_independent_state_and_returns_to_main() {
  let root = TempRoot::new("branch_attach");
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.path().to_path_buf(),
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
      archive_num_retained: 100,
    })
    .unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"main-a".to_vec(), 1)],
      )
      .await
      .unwrap();
    runtime.create_branch(&tbl, "Feature").await.unwrap();

    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "feature", 2).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    runtime
      .put(
        &tbl,
        vec![
          BatchEntry::upsert(b"a".to_vec(), b"branch-a".to_vec(), 2),
          BatchEntry::upsert(b"c".to_vec(), b"branch-c".to_vec(), 2),
        ],
      )
      .await
      .unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );

    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "main", 3).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"main-a".to_vec())
    );
    assert_eq!(runtime.get(&tbl, b"c").await.unwrap(), None);

    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "feature", 4).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"branch-a".to_vec())
    );
    assert_eq!(
      runtime.get(&tbl, b"c").await.unwrap(),
      Some(b"branch-c".to_vec())
    );
    runtime.shutdown().await.unwrap();
  });
}

fn config(
  root: &TempRoot,
  pages_per_file_shift: u8,
  large_value_threshold: usize,
) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root.path().to_path_buf());
  config.pages_per_file_shift = pages_per_file_shift;
  config.large_value_threshold = large_value_threshold;
  config
}

fn up(key: &[u8], value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)
}

/// pooled-09：`ReopenArchive` 是 `SKIPS_OPEN`，不会先走 `open_branch`。
/// 归档重开后 branch 必须恢复预留上限，块内写继续成功、父表数据不受影响。
#[test]
fn reopen_archive_keeps_branch_writable_after_reattach() {
  let root = TempRoot::new("branch_reopen_archive");
  pollster::block_on(async {
    // shift = 18：分支块远大于测试数据，验证归档重开后的正常写入不被误拒。
    let runtime = Runtime::start_mem(config(&root, 18, 1024)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();

    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl, vec![up(b"a", b"A", 1)]).await.unwrap();
    runtime.create_branch(&tbl, "feature").await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "feature", 2).await.unwrap();
    runtime.put(&tbl, vec![up(b"f", b"F", 2)]).await.unwrap();
    runtime.create_archive(&tbl, "t1", 4).await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "main", 3).await.unwrap();
    runtime.put(&tbl, vec![up(b"m", b"MAIN", 3)]).await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "feature", 4).await.unwrap();
    runtime.reopen_archive(&tbl, "t1").await.unwrap();
    runtime
      .put(&tbl, vec![up(b"f2", b"F2", 4)])
      .await
      .expect("归档重开后的块内写必须成功");
    assert_eq!(runtime.get(&tbl, b"f").await.unwrap(), Some(b"F".to_vec()));
    assert_eq!(
      runtime.get(&tbl, b"f2").await.unwrap(),
      Some(b"F2".to_vec())
    );
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "main", 5).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec()),
      "branch 归档重开后的写覆盖了父表数据"
    );
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    assert_eq!(runtime.get(&tbl, b"f2").await.unwrap(), None);
    runtime.shutdown().await.unwrap();
  });
}

/// pooled-09：branch 写满自己的预留块后归档、重新 attach 并
/// `reopen_archive(tag)`，上限必须从 manifest 恢复——越过块末尾的写批次返回
/// `OutOfSpace`，不能因为 `branch_file_limit == None` 而写穿预留块。
#[test]
fn reopen_archive_rejects_write_when_branch_block_full() {
  let root = TempRoot::new("branch_reopen_archive_full");
  pollster::block_on(async {
    // shift = 0：每文件 1 页，预留块 1024 页，块内写满可在测试时长内完成。
    let runtime = Runtime::start_mem(config(&root, 0, 4096)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();

    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl, vec![up(b"a", b"A", 1)]).await.unwrap();
    runtime.create_branch(&tbl, "feature").await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "feature", 2).await.unwrap();
    let mut full = false;
    for round in 0..200u32 {
      let batch: Vec<BatchEntry> = (0..100u32)
        .map(|index| {
          let key = format!("k{:05}", round * 100 + index).into_bytes();
          BatchEntry::upsert(key, vec![b'V'; 300], 2)
        })
        .collect();
      match runtime.put(&tbl, batch).await {
        Ok(()) => {}
        Err(KvError::OutOfSpace) => {
          full = true;
          break;
        }
        Err(err) => panic!("意外的写入错误: {err:?}"),
      }
    }
    assert!(full, "未能写满 branch 预留块（布局变化？）");
    runtime.create_archive(&tbl, "t1", 4).await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "main", 3).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "main 在 branch 创建后必须保留原值"
    );
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "feature", 4).await.unwrap();
    runtime.reopen_archive(&tbl, "t1").await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"k00000").await.unwrap(),
      Some(vec![b'V'; 300]),
      "归档快照的既有数据必须可读"
    );
    // 归档保留的 max 距块末尾只剩几页：一批新 key 必然越过上限。修复后上限
    // 已从 manifest 恢复，`write_batch` 结算点的检查返回 OutOfSpace；修复前
    // `branch_file_limit == None` 整段跳过，写穿预留块并落到父表页上。
    let overflow: Vec<BatchEntry> = (0..100u32)
      .map(|index| {
        let key = format!("after{index:03}").into_bytes();
        BatchEntry::upsert(key, vec![b'X'; 300], 4)
      })
      .collect();
    assert_eq!(
      runtime.put(&tbl, overflow).await.unwrap_err(),
      KvError::OutOfSpace,
      "归档重开后 branch 上限缺失，写穿了预留块"
    );
    runtime.detach_shard(0).await.unwrap();

    // 越界失败批会先 flush 再回滚（pooled-08 的已知残余），可能占用父表刚用过的
    // file page id；这里只校验位于 file 0 的父表 key 未被波及。
    runtime.attach_shard(0, "main", 5).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    runtime.shutdown().await.unwrap();
  });
}

/// sem-25：main 建 branch 后继续写，branch 再建孙分支。
///
/// 孙块基址必须避开 main 在 fork 后的写区间；修复前 `g` 的块与 main 的
/// post-fork 写同块，`g` 写入后 main 的 100 条批量与 fork 前 key 全部丢失。
/// 这里走 `Runtime::start_mem`（MemStore，默认 Pooled），与 Store 级
/// Pooled / Append 用例互补。
#[test]
fn nested_branch_after_ancestor_write_keeps_data() {
  let root = TempRoot::new("branch_sem25_nested");
  pollster::block_on(async {
    let runtime = Runtime::start_mem(config(&root, 2, 1024)).unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    let value = vec![b'V'; 200];
    let pad = |prefix: &str, ts: u64| -> Vec<BatchEntry> {
      (0..100u32)
        .map(|index| {
          BatchEntry::upsert(
            format!("{prefix}{index:03}").into_bytes(),
            value.clone(),
            ts + u64::from(index),
          )
        })
        .collect()
    };

    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl, vec![up(b"a", b"A", 1)]).await.unwrap();
    runtime.create_branch(&tbl, "f1").await.unwrap();
    runtime.put(&tbl, pad("m", 2)).await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "f1", 2).await.unwrap();
    runtime.create_branch(&tbl, "g").await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    runtime.attach_shard(0, "g", 3).await.unwrap();
    runtime.put(&tbl, pad("g", 3)).await.unwrap();
    runtime.detach_shard(0).await.unwrap();

    // main 重新 attach：fork 前的 key 与 fork 后的 100 条批量都必须完好。
    runtime.attach_shard(0, "main", 4).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"a").await.unwrap(),
      Some(b"A".to_vec()),
      "fork 前的根数据被孙分支覆盖"
    );
    assert_pad(&runtime, &tbl, "m").await;
    runtime.detach_shard(0).await.unwrap();

    // 孙分支自己的数据也完好。
    runtime.attach_shard(0, "g", 5).await.unwrap();
    assert_pad(&runtime, &tbl, "g").await;
    runtime.shutdown().await.unwrap();
  });
}

/// sem-25 用例的 100 条批量校验。
async fn assert_pad(runtime: &Runtime, tbl: &TableIdent, prefix: &str) {
  let value = vec![b'V'; 200];
  for index in 0..100u32 {
    let key = format!("{prefix}{index:03}").into_bytes();
    assert_eq!(
      runtime.get(tbl, &key).await.unwrap(),
      Some(value.clone()),
      "数据丢失 prefix={prefix} key={key:?}"
    );
  }
}
