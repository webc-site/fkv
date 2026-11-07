//! data store shard 生命周期（sto-61）：attach / detach 的幂等与非法转换。
//!
//! 对齐 C++ `AttachShard` / `DetachShard` 的状态要求：已托管时换 shard / branch
//! 必须先 detach；`Detached` 下 detach 幂等；旧 term 重新 attach 让整 shard 过期。
use std::{
  fs::{read_dir, read_to_string},
  path::{Path, PathBuf},
};

use fkv_api::{Request, Response};
use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_shard::{Backend, Shard, ShardConfig};
use fkv_store::AllocMode;
use fkv_testkit::TempRoot;
use fkv_types::{StoreMode, TableIdent, current_term_file_name};

fn config(root: PathBuf) -> ShardConfig {
  ShardConfig {
    alloc_mode: AllocMode::Append,
    shard_id: 0,
    root,
    page_size: 512,
    restart_interval: 16,
    index_restart_interval: 16,
    pages_per_file_shift: 18,
    segment_size: 4096,
    segments_per_file_shift: 2,
    large_value_threshold: 1024,
    fixed_buffer_size: 0,
    queue_size: 64,
    auto_reopen_retry_times: 0,
    auto_reopen_pending_time_us: 0,
    oom_retry_times: 0,
    compression_level: None,
    verify_checksum: true,
    overflow_pointers: fkv_page::DEFAULT_OVERFLOW_POINTERS,
    file_amplify_factor: 0,
    segment_file_amplify_factor: 0,
    manifest_limit: 8 * 1024 * 1024,
    max_write_batch_pages: 32,
    max_inflight_write: 32 * 1024,
    fd_limit: 10_000,
    index_cache_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    index_cache_total_bytes: fkv_config::DEFAULT_INDEX_CACHE_BYTES as usize,
    root_meta_cache_bytes: 0,
    io_queue_size: 4096,
    backend: Backend::default(),
    mode: StoreMode::Local,
    standby: None,
    max_write_concurrency: 0,
    max_processing_time_us: 0,
    yield_budget_us: 0,
  }
}

fn attach(branch: &str, shard_id: u32, term: u64) -> Request {
  Request::AttachShard {
    branch: branch.to_owned(),
    shard_id,
    term,
  }
}

fn detach(shard_id: u32) -> Request {
  Request::DetachShard { shard_id }
}

fn put(tbl: &TableIdent, key: &[u8], value: &[u8], term: u64) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)],
    term: Some(term),
  }
}

fn get(tbl: &TableIdent, key: &[u8]) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key.to_vec(),
  }
}

/// 列出表目录下现存的 data 文件 id（文件名 `data_<id>_<branch>_<term>`，
/// branch 名可含 `-`，故只取 `data_` 之后到下一个 `_` 之间的段）。
fn data_file_id_li(root: &Path, tbl: &TableIdent) -> Vec<u32> {
  let dir = root.join(format!("{}.{}", tbl.tbl_name, tbl.partition_id));
  let mut li = read_dir(&dir)
    .unwrap_or_else(|err| panic!("read_dir {dir:?} failed: {err:?}"))
    .filter_map(|entry| {
      let name = entry.ok()?.file_name().into_string().ok()?;
      let id = name.strip_prefix("data_")?.split('_').next()?.to_owned();
      id.parse::<u32>().ok()
    })
    .collect::<Vec<_>>();
  li.sort_unstable();
  li
}

/// 未 attach 拒绝普通请求；同 term 重复 attach 幂等且不驱逐已写数据。
#[compio::test]
async fn attach_is_idempotent_and_keeps_state() {
  let root = TempRoot::new("lifecycle");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("lifecycle", 1).unwrap();

  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::NotRunning
  );
  assert!(matches!(
    shard.request(attach("main", 0, 7)).await.unwrap(),
    Response::AttachShard
  ));
  shard.request(put(&tbl, b"a", b"A", 7)).await.unwrap();
  // 重复 attach 不得重置 store 状态（否则需要重新打开 manifest）。
  assert!(matches!(
    shard.request(attach("main", 0, 7)).await.unwrap(),
    Response::AttachShard
  ));
  assert!(matches!(
    shard.request(get(&tbl, b"a")).await.unwrap(),
    Response::Get(Some(value)) if value == b"A"
  ));
  // detach 幂等：第二次直接返回成功。
  assert!(matches!(
    shard.request(detach(0)).await.unwrap(),
    Response::DetachShard
  ));
  assert!(matches!(
    shard.request(detach(0)).await.unwrap(),
    Response::DetachShard
  ));
  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::NotRunning
  );
  shard.shutdown().await.unwrap();
}

/// 已托管时换 branch / shard 是非法转换；旧 term 让整 shard 过期。
#[compio::test]
async fn illegal_transitions_are_rejected() {
  let root = TempRoot::new("lifecycle");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("demo", 1).unwrap();

  shard.request(attach("main", 0, 5)).await.unwrap();
  assert_eq!(
    shard.request(attach("feature", 0, 5)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    shard.request(attach("main", 1, 5)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    shard.request(attach("main", 0, 6)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  assert_eq!(
    shard.request(detach(1)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  // 旧 term attach：整 shard 过期，普通请求快速失败。
  assert_eq!(
    shard.request(attach("main", 0, 4)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  // 同 term 重新 attach 恢复。
  shard.request(attach("main", 0, 5)).await.unwrap();
  shard.request(put(&tbl, b"a", b"A", 5)).await.unwrap();
  assert!(matches!(
    shard.request(get(&tbl, b"a")).await.unwrap(),
    Response::Get(Some(value)) if value == b"A"
  ));
  shard.shutdown().await.unwrap();
}

/// Local 模式 attach 落 `CURRENT_TERM_<branch>_<shard_id>`；detach 清空运行期
/// 身份但保留持久 term，旧 term 的重新 attach 仍由 CAS 拒绝；换 term 后旧数据可读，
/// 新写入落在 bump 之后的文件（与 core-22a 的 allocator bump 配合）。
#[compio::test]
async fn local_term_file_fences_and_detach_resets_state() {
  let root = TempRoot::new("lifecycle");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("term", 1).unwrap();
  let term_file = root.join(current_term_file_name("main", 0));

  shard.request(attach("main", 0, 3)).await.unwrap();
  assert_eq!(read_to_string(&term_file).unwrap(), "3");
  shard.request(put(&tbl, b"a", b"A", 3)).await.unwrap();

  shard.request(detach(0)).await.unwrap();
  // detach 不删持久 term：跨进程 / 重连的 fencing 依据仍在。
  assert_eq!(read_to_string(&term_file).unwrap(), "3");
  // 运行期身份归零：普通请求快速失败。
  assert_eq!(
    shard.request(get(&tbl, b"a")).await.unwrap_err(),
    KvError::NotRunning
  );
  // detach 后旧 term 重新 attach：由持久 term 文件 CAS 拒绝。
  assert_eq!(
    shard.request(attach("main", 0, 2)).await.unwrap_err(),
    KvError::ExpiredTerm
  );

  // 更高 term 接管：term 文件更新，旧 term 写入失败。
  shard.request(attach("main", 0, 4)).await.unwrap();
  assert_eq!(read_to_string(&term_file).unwrap(), "4");
  // 跨 term 读：旧 term 写入的数据仍可见。
  assert!(matches!(
    shard.request(get(&tbl, b"a")).await.unwrap(),
    Response::Get(Some(value)) if value == b"A"
  ));
  assert_eq!(
    shard.request(put(&tbl, b"a", b"A4", 3)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  // 旧 term 请求让整 shard 过期，重挂同 term 恢复后可继续写。
  shard.request(attach("main", 0, 4)).await.unwrap();
  shard.request(put(&tbl, b"b", b"B", 4)).await.unwrap();
  assert!(matches!(
    shard.request(get(&tbl, b"b")).await.unwrap(),
    Response::Get(Some(value)) if value == b"B"
  ));

  shard.shutdown().await.unwrap();
}

/// sto-69b：shard 身份与存储布局在启动时固定——请求侧不能改绑 shard id，
/// `Gc` / `Compact` 的 `pages_per_file_shift` 必须等于 `ShardConfig` 的值。
#[compio::test]
async fn attach_id_and_layout_shift_are_validated() {
  let root = TempRoot::new("lifecycle");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("layout", 1).unwrap();

  // 配置 shard_id = 0：别的 id 不能在同一个 shard 上 attach / detach。
  assert_eq!(
    shard.request(attach("main", 1, 1)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  shard.request(attach("main", 0, 1)).await.unwrap();
  assert_eq!(
    shard.request(detach(1)).await.unwrap_err(),
    KvError::InvalidArgs
  );
  shard.request(put(&tbl, b"a", b"A", 1)).await.unwrap();

  // Compact 的布局位宽由调用方传入：错误位宽会让 file_id 换算错位，必须拒绝。
  assert_eq!(
    shard
      .request(Request::Compact {
        tbl: tbl.clone(),
        file_id: 0,
        pages_per_file_shift: 64,
      })
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );
  assert!(matches!(
    shard
      .request(Request::FileGc { tbl: tbl.clone() })
      .await
      .unwrap(),
    Response::FileGc(_)
  ));
  assert!(matches!(
    shard.request(Request::LocalGc { tbl }).await.unwrap(),
    Response::LocalGc(_)
  ));

  shard.shutdown().await.unwrap();
}

/// `ShardLifecycle::is_attached` 是 runtime 侧唯一的生命周期探针
/// （`get()` 已私有化）：attach / detach 必须让它翻转，过期后不再报托管。
#[compio::test]
async fn lifecycle_probe_tracks_attach_detach_and_expire() {
  let root = TempRoot::new("lifecycle");
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("probe", 1).unwrap();
  assert!(!shard.lifecycle().is_attached(), "启动时未托管");

  shard.request(attach("main", 0, 1)).await.unwrap();
  assert!(shard.lifecycle().is_attached());

  shard.request(put(&tbl, b"a", b"A", 1)).await.unwrap();
  // 旧 term 写让整 shard 过期：探针必须翻成非托管。
  assert_eq!(
    shard.request(put(&tbl, b"b", b"B", 0)).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert!(!shard.lifecycle().is_attached(), "过期后不再托管");

  shard.shutdown().await.unwrap();
}

/// sem-64：公开 `Request::Compact` 是**单文件** data 搬迁原语，不是 C++
/// `CompactRequest` 那种全表 `Compact()`（data + segment + 末尾 `RunFileGc`）。
/// 请求只搬 `file_id` 指定的文件，且**不带 GC**：被搬空的旧文件要等下一个
/// 显式 `FileGc` 请求才回收；另一个同样有垃圾页的文件则完全不动。该差异
/// 已按有意差异登记（COMPAT_BASELINE「Compact 公开请求形态」），本用例锁
/// 现行为，防止请求面被无意改成全表语义。三步断言（Local 模式直接看盘上
/// 文件集）：
/// 1. `Compact{file_id: 0}` 返回搬迁页数 > 0，且 `data_0_*` 仍在盘上
///    （compact 不回收文件——若请求被改成 `maintain` 语义，其中的内联
///    `file_gc` 会当场删掉它）；
/// 2. 随后 `FileGc` 只回收被搬空的 file 0：file 1 的 live 页原位未动
///    （若请求被改成「搬全部文件 + GC」，file 1 的 live 页也会被搬走，
///    `data_1_*` 一并被删）；
/// 3. 改写键读回新值、未改写键读回旧值（搬迁只换物理页，不改逻辑内容）。
#[compio::test]
async fn compact_request_moves_only_named_file_and_defers_gc() {
  let root = TempRoot::new("sem64-compact-shape");
  let mut cfg = config(root.path().to_path_buf());
  // 8 页 / 文件：24 个 400B value（512B 页下一页一条）铺出 ≥3 个 data 文件。
  cfg.pages_per_file_shift = 3;
  let shard = Shard::start(cfg).unwrap();
  shard.request(attach("main", 0, 1)).await.unwrap();
  let tbl = TableIdent::new("sem64compact", 1).unwrap();

  // a00..a23 顺序铺页：file 0 ≈ a00..a07、file 1 ≈ a08..a15（中间夹少量
  // 内节点页，页号边界不影响断言）。file_amplify_factor = 0：写路径不自动
  // compact / GC，盘上文件集只由本测试的显式请求驱动。
  for round in 0..3u32 {
    let batch_li = (0..8)
      .map(|index| {
        let key = format!("a{:02}", round * 8 + index);
        BatchEntry::upsert(key.into_bytes(), vec![b'x'; 400], 1)
      })
      .collect();
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li,
        term: Some(1),
      })
      .await
      .unwrap();
  }
  // 两个文件各改写一小段（新值追加到尾部）：file 0 与 file 1 都留下垃圾页，
  // 也都仍有 live 页。
  for start in [0u32, 8] {
    let batch_li = (start..start + 4)
      .map(|index| {
        let key = format!("a{index:02}");
        BatchEntry::upsert(key.into_bytes(), vec![b'y'; 400], 2)
      })
      .collect();
    shard
      .request(Request::Put {
        tbl: tbl.clone(),
        batch_li,
        term: Some(1),
      })
      .await
      .unwrap();
  }
  let files_before = data_file_id_li(root.path(), &tbl);
  assert!(
    files_before.contains(&0) && files_before.contains(&1),
    "夹具应铺出 ≥2 个 data 文件：{files_before:?}"
  );

  let moved = match shard
    .request(Request::Compact {
      tbl: tbl.clone(),
      file_id: 0,
      pages_per_file_shift: 3,
    })
    .await
    .unwrap()
  {
    Response::Compact(moved) => moved,
    other => panic!("unexpected compact response: {other:?}"),
  };
  assert!(moved > 0, "file 0 有 live 页，应至少搬走一页");
  assert!(
    data_file_id_li(root.path(), &tbl).contains(&0),
    "compact 不带 GC：被搬空的 data_0 必须仍在盘上"
  );

  let removed = match shard
    .request(Request::FileGc { tbl: tbl.clone() })
    .await
    .unwrap()
  {
    Response::FileGc(removed) => removed,
    other => panic!("unexpected file gc response: {other:?}"),
  };
  assert!(removed >= 1);
  let files_after = data_file_id_li(root.path(), &tbl);
  assert!(
    !files_after.contains(&0),
    "被搬空的 file 0 应由 FileGc 回收：{files_after:?}"
  );
  assert!(
    files_after.contains(&1),
    "未请求 compact 的 file 1 不应被搬空 / 回收：{files_after:?}"
  );

  assert!(matches!(
    shard.request(get(&tbl, b"a00")).await.unwrap(),
    Response::Get(Some(value)) if value.iter().all(|byte| *byte == b'y')
  ));
  assert!(matches!(
    shard.request(get(&tbl, b"a05")).await.unwrap(),
    Response::Get(Some(value)) if value.iter().all(|byte| *byte == b'x')
  ));
  assert!(matches!(
    shard.request(get(&tbl, b"a12")).await.unwrap(),
    Response::Get(Some(value)) if value.iter().all(|byte| *byte == b'x')
  ));

  shard.shutdown().await.unwrap();
}
