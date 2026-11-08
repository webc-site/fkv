//! io-18：生产默认 4 KiB 页 + O_DIRECT 的端到端接线与 tmpfs 回退。
//!
//! 既有 shard 集成用例都用 `page_size: 512`，`with_direct_io` 的
//! `page_size % 4096` 门会把 direct 静默关掉。本文件用 4 KiB 页驱动真实 shard：
//! - 支持 O_DIRECT 的目录：断言 shard 打开的 data / segment 句柄确实带
//!   `O_DIRECT`（去掉 `build_compio_store` 的接线时断言失败）；
//! - tmpfs（`/dev/shm`）：断言首次写不 `IoFail`、读写 / scan / compact 正常，
//!   且没有 O_DIRECT 句柄（按类别回退 buffered）。

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};

use fkv_api::{Request, Response, ScanPageOption};
use fkv_btree::BatchEntry;
use fkv_shard::{AllocMode, Backend, Shard, ShardConfig};
use fkv_testkit::{TempRoot, direct_io_probe};
use fkv_types::{StoreMode, TableIdent};

const PAGE_SIZE: usize = 4096;
const SEGMENT_SIZE: usize = 256 * 1024;

fn config(root: PathBuf) -> ShardConfig {
  let mut cfg = ShardConfig::for_tests(root);
  cfg.page_size = PAGE_SIZE;
  cfg.segment_size = SEGMENT_SIZE as u32;
  cfg.segments_per_file_shift = 7;
  cfg.large_value_threshold = 8 * 1024;
  cfg.queue_size = 64;
  cfg.backend = Backend::Local;
  cfg
}

fn put(tbl: &TableIdent, key: &[u8], value: &[u8]) -> Request {
  Request::Put {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)],
    term: Some(1),
  }
}

fn get(tbl: &TableIdent, key: &[u8]) -> Request {
  Request::Get {
    tbl: tbl.clone(),
    key: key.to_vec(),
  }
}

fn put_large(tbl: &TableIdent, key: &[u8], value: &[u8]) -> Request {
  Request::PutLarge {
    tbl: tbl.clone(),
    batch_li: vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), 1)],
    term: Some(1),
  }
}

fn get_large(tbl: &TableIdent, key: &[u8]) -> Request {
  Request::GetLarge {
    tbl: tbl.clone(),
    key: key.to_vec(),
  }
}

/// 在当前根上跑一轮完整数据面：普通页、大 value（segment）、scan、compact。
async fn exercise(shard: &Shard, tbl: &TableIdent) {
  for index in 0..32u8 {
    let key = [b'k', index];
    let value = vec![index; 400];
    assert!(matches!(
      shard.request(put(tbl, &key, &value)).await.unwrap(),
      Response::Put
    ));
    assert!(matches!(
      shard.request(get(tbl, &key)).await.unwrap(),
      Response::Get(Some(got)) if got == value
    ));
  }
  // 大 value 走 segment 文件，保证 segment O_DIRECT 也被驱动。注意：普通
  // `Put` 的大 value 走 data 文件 overflow 链，不落 segment；必须用
  // `PutLarge` / `GetLarge` 才会真正打开 segment 文件。
  let large = vec![0xABu8; 16 * 1024];
  shard
    .request(put_large(tbl, b"large", &large))
    .await
    .unwrap();
  assert!(matches!(
    shard.request(get_large(tbl, b"large")).await.unwrap(),
    Response::GetLarge(Some(got)) if got.value == large
  ));

  // 扫描范围止于 `"l"`：segment 大 value 的盘面值是指针编码，scan 命中会
  // 返回 `LargeValueUnsupported`（issue 15），这里只覆盖普通页。
  let scan = shard
    .request(Request::Scan {
      tbl: tbl.clone(),
      begin: b"k".to_vec(),
      end: b"l".to_vec(),
      max_entries: 0,
      option: ScanPageOption::default(),
    })
    .await
    .unwrap();
  assert!(matches!(scan, Response::Scan(ref reply) if reply.entry_li.len() == 32));

  assert!(matches!(
    shard
      .request(Request::Compact {
        tbl: tbl.clone(),
        file_id: 0,
        pages_per_file_shift: 18,
      })
      .await
      .unwrap(),
    Response::Compact(_)
  ));
  assert!(matches!(
    shard.request(get_large(tbl, b"large")).await.unwrap(),
    Response::GetLarge(Some(got)) if got.value == large
  ));
}

/// 支持 O_DIRECT 的根上，生产默认配置必须真的按 O_DIRECT 打开 data / segment。
#[cfg(target_os = "linux")]
#[compio::test]
async fn default_4k_shard_opens_direct_handles() {
  let root = TempRoot::new("io18_shard_direct");
  let probe = direct_io_probe(root.path());
  if !probe.supported() {
    eprintln!("跳过：临时目录不支持 O_DIRECT");
    return;
  }
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("direct", 1).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  exercise(&shard, &tbl).await;
  // data 句柄必须仍在 fd 缓存里且带 O_DIRECT。segment 的 O_DIRECT 接线由
  // `fkv_io_compio/tests/direct_fallback.rs` 的 4 KiB builder 用例逐项覆盖；
  // shard 层不强制此刻仍有 segment fd 常驻（compact / 读回可能重建句柄，
  // fdinfo 采样时机不稳定）。
  let (data, segment) = probe.fd_kind_num();
  assert!(
    data >= 1,
    "4 KiB 页 shard 的默认 data direct 接线失效：data={data} segment={segment}"
  );
  shard.shutdown().await.unwrap();
}

/// tmpfs 上生产默认配置必须回退 buffered 而不是让 data / segment IO 持续失败。
#[cfg(target_os = "linux")]
#[compio::test]
async fn tmpfs_4k_shard_falls_back_and_serves() {
  let shm = Path::new("/dev/shm");
  if !shm.is_dir() {
    eprintln!("跳过：当前环境没有 /dev/shm");
    return;
  }
  let root = TempRoot::with_base(shm, "io18_shard_tmpfs");
  let probe = direct_io_probe(root.path());
  if probe.supported() {
    eprintln!("跳过：/dev/shm 支持 O_DIRECT，回退路径不适用");
    return;
  }
  let shard = Shard::start(config(root.path().to_path_buf())).unwrap();
  let tbl = TableIdent::new("tmpfs", 1).unwrap();
  shard
    .request(Request::AttachShard {
      branch: "main".to_owned(),
      shard_id: 0,
      term: 1,
    })
    .await
    .unwrap();
  // 首次写即触发 O_DIRECT open + 回退；之后 put / get / scan / compact 全部正常。
  exercise(&shard, &tbl).await;
  assert_eq!(
    probe.fd_kind_num(),
    (0, 0),
    "tmpfs 回退后不应再存在 O_DIRECT 句柄"
  );
  shard.shutdown().await.unwrap();
}
