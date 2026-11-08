#![cfg(target_os = "linux")]

use std::pin::pin;

use fkv_error::KvError;
use fkv_io::{FixedSegmentIo, FixedSegmentRead, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use futures::poll;

fn pattern(len: usize) -> Vec<u8> {
  (0..len).map(|index| (index % 251) as u8).collect()
}

#[compio::test]
async fn fixed_buffer_ops_complete_on_runtime_ring() {
  // `TempRoot` 析构时递归清理，无需手工 `remove_dir_all`。
  let root = TempRoot::new("fixed");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  let mut src = store.register_fixed_buffer(4096).unwrap();
  let mut dst = store.register_fixed_buffer(4096).unwrap();
  assert!(src.is_registered());
  assert!(dst.is_registered());
  assert_eq!(src.index(), 0);
  assert_eq!(dst.index(), 1);

  let value = pattern(4096);
  src
    .as_mut_slice()
    .expect("无在飞 fixed op")
    .copy_from_slice(&value);
  store
    .write_segment_fixed(&tbl, 0, &src, 0, 4096)
    .await
    .unwrap();

  // 普通读确认 WRITE_FIXED 真正落盘。
  let mut plain = vec![0u8; 4096];
  store.read_segment(&tbl, 0, &mut plain).await.unwrap();
  assert_eq!(plain, value);

  // READ_FIXED 通过 compio completion queue 完成后，数据直接落在注册 buffer。
  dst.as_mut_slice().expect("无在飞 fixed op").fill(0);
  store
    .read_segment_fixed(&tbl, 0, &mut dst, 0, 4096)
    .await
    .unwrap();
  assert_eq!(dst.as_slice(), value);

  // 非零 buffer 偏移 + 另一个 segment 文件。
  store
    .write_segment_fixed(&tbl, 4, &src, 100, 512)
    .await
    .unwrap();
  let mut out = vec![0u8; 512];
  store.read_segment(&tbl, 4, &mut out).await.unwrap();
  assert_eq!(out, value[100..612]);
}

#[compio::test]
async fn fixed_reads_interleave_with_other_compio_io() {
  let root = TempRoot::new("fixed_interleave");
  let tbl = TableIdent::new("demo", 2).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2)
  .with_checksum_verify(false);
  let mut src = store.register_fixed_buffer(4096).unwrap();
  let mut dst = store.register_fixed_buffer(4096).unwrap();
  let value = pattern(4096);
  src
    .as_mut_slice()
    .expect("无在飞 fixed op")
    .copy_from_slice(&value);
  store
    .write_segment_fixed(&tbl, 0, &src, 0, 4096)
    .await
    .unwrap();

  // 在 fixed read 之间插入普通 compio 文件 IO，两者共享同一个 runtime ring。
  let mut page = vec![0u8; 512];
  store.write_page(&tbl, 0, &page).await.unwrap();
  page[0] = 7;
  store.write_page(&tbl, 0, &page).await.unwrap();
  store
    .read_segment_fixed(&tbl, 0, &mut dst, 0, 4096)
    .await
    .unwrap();
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 7);
  assert_eq!(dst.as_slice(), value);
}

/// io-37：外层 future 在 `WRITE_FIXED` 在飞时被 drop，槽位必须等完成确认
/// （driver 收割 CQE）后才归还；提前归还 + `BUFFERS_UPDATE` 覆盖会让旧
/// SQE 把新缓冲内容写到旧 offset。
#[compio::test]
async fn dropped_fixed_write_keeps_its_slot_until_completion() {
  let root = TempRoot::new("fixed_cancel");
  let tbl = TableIdent::new("demo", 3).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);

  // 预热：建目录 / 打开并缓存 segment 文件，使下面的首次 poll 直达固定写提交。
  store.write_segment(&tbl, 0, &pattern(4096)).await.unwrap();

  let mut src = store.register_fixed_buffer(4096).unwrap();
  assert_eq!(src.index(), 0);
  let old = pattern(4096);
  src
    .as_mut_slice()
    .expect("无在飞 fixed op")
    .copy_from_slice(&old);

  {
    let mut write = pin!(store.write_segment_fixed(&tbl, 0, &src, 0, 4096));
    assert!(
      poll!(write.as_mut()).is_pending(),
      "WRITE_FIXED 应在首次 poll 挂起"
    );
  }

  // 取消后用户侧归还：在飞 SQE 未收割前 index 0 不得复用。
  drop(src);
  let fresh = store.register_fixed_buffer(4096).unwrap();
  assert_ne!(fresh.index(), 0, "在飞 SQE 完成前槽位不得复用");

  // 新槽位写另一个 segment；旧 segment 不得被新缓冲内容污染。
  let new = pattern(4096)
    .iter()
    .map(|byte| byte.wrapping_add(1))
    .collect::<Vec<_>>();
  let mut fresh = fresh;
  fresh
    .as_mut_slice()
    .expect("无在飞 fixed op")
    .copy_from_slice(&new);
  store
    .write_segment_fixed(&tbl, 1, &fresh, 0, 4096)
    .await
    .unwrap();

  let mut plain = vec![0u8; 4096];
  store.read_segment(&tbl, 0, &mut plain).await.unwrap();
  assert_eq!(plain, old, "旧 offset 被新缓冲内容污染");
}

/// io-38：O_DIRECT 下整段 `WRITE_FIXED` 的段内偏移必须按页对齐。违规时提交前
/// 返回 `InvalidArgs`，不能把内核 EINVAL 摊平成 `IoFail`；对齐全段照常落盘。
#[compio::test]
async fn direct_fixed_write_rejects_misaligned_offset() {
  let root = TempRoot::new("fixed_direct_align");
  let tbl = TableIdent::new("demo", 4).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    4096,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2)
  .with_direct_io(true);
  let mut buf = store.register_fixed_buffer(8192).unwrap();
  let value = pattern(4096);
  buf.as_mut_slice().expect("无在飞 fixed op")[..4096].copy_from_slice(&value);

  // 段内偏移 512 非页对齐，整段长度也救不回来。
  let result = store.write_segment_fixed(&tbl, 0, &buf, 512, 4096).await;
  assert_eq!(result, Err(KvError::InvalidArgs));

  // 对齐偏移 0 走真正的 WRITE_FIXED，数据往返一致。
  store
    .write_segment_fixed(&tbl, 0, &buf, 0, 4096)
    .await
    .unwrap();
  let mut plain = vec![0u8; 4096];
  store.read_segment(&tbl, 0, &mut plain).await.unwrap();
  assert_eq!(plain, value);
}

/// io-38：未开 O_DIRECT 时没有页对齐约束，非对齐偏移 + 整段长度照常写成功
/// （护栏只拦 O_DIRECT 路径，不收紧 buffered 语义）。
#[compio::test]
async fn buffered_fixed_write_allows_misaligned_offset() {
  let root = TempRoot::new("fixed_buffered_align");
  let tbl = TableIdent::new("demo", 5).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    4096,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  let mut buf = store.register_fixed_buffer(8192).unwrap();
  let value = pattern(4096);
  buf.as_mut_slice().expect("无在飞 fixed op")[..4096].copy_from_slice(&value);

  // 写的是 `buf[512..4608]`，读回整段应与该区间一致。
  let expected = buf.as_slice()[512..4608].to_vec();
  store
    .write_segment_fixed(&tbl, 0, &buf, 512, 4096)
    .await
    .unwrap();
  let mut plain = vec![0u8; 4096];
  store.read_segment(&tbl, 0, &mut plain).await.unwrap();
  assert_eq!(plain, expected);
}

/// rev-44 残余计划 1：批量 fixed 读在注册缓冲上并发提交——一次调用 12 项
/// （超过 `MAX_SEGMENTS_BATCH`），同槽位多个在飞 READ_FIXED 由 `SlotLease`
/// 在飞计数与 `share_disjoint` 区间租约看护，各区间数据全部正确落位。
#[compio::test]
async fn fixed_batch_read_submits_concurrent_ops() {
  let root = TempRoot::new("fixed_batch_read");
  let tbl = TableIdent::new("demo", 6).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);

  // 12 个 segment 各写一份可区分内容（每段 4 KiB，跨 3 个 segment 文件）。
  let segment_num = 12u64;
  let content_li: Vec<Vec<u8>> = (0..segment_num)
    .map(|segment_fp_id| {
      (0..4096usize)
        .map(|index| (index % 251) as u8 ^ segment_fp_id as u8)
        .collect()
    })
    .collect();
  for (segment_fp_id, content) in content_li.iter().enumerate() {
    store
      .write_segment(&tbl, segment_fp_id as u64, content)
      .await
      .unwrap();
  }

  let value_len = segment_num as usize * 4096;
  let mut dst = store.register_fixed_buffer(value_len + 4096).unwrap();
  assert!(dst.is_registered());
  dst.as_mut_slice().expect("无在飞 fixed op").fill(0xEE);

  let item_li: Vec<FixedSegmentRead> = (0..segment_num)
    .map(|segment_fp_id| FixedSegmentRead {
      segment_fp_id,
      offset: segment_fp_id as usize * 4096,
      len: 4096,
    })
    .collect();
  store
    .read_segment_fixed_li(&tbl, &item_li, &mut dst)
    .await
    .unwrap();

  let view = dst.as_slice();
  for (segment_fp_id, content) in content_li.iter().enumerate() {
    let offset = segment_fp_id * 4096;
    assert_eq!(&view[offset..offset + 4096], content);
  }
  // 保留区不被改写。
  assert!(view[value_len..].iter().all(|byte| *byte == 0xEE));
}

/// rev-44 残余计划 1：注册缓冲上的批量 fixed 读在提交前整批拒绝重叠 /
/// 超段长计划——并发 READ_FIXED 的重叠区间是内核并发写同一字节的真竞争。
#[compio::test]
async fn fixed_batch_read_rejects_overlap_on_registered_slot() {
  let root = TempRoot::new("fixed_batch_overlap");
  let tbl = TableIdent::new("demo", 7).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  store.write_segment(&tbl, 0, &pattern(4096)).await.unwrap();
  store.write_segment(&tbl, 1, &pattern(4096)).await.unwrap();

  let mut buf = store.register_fixed_buffer(4096 * 2).unwrap();
  buf.as_mut_slice().expect("无在飞 fixed op").fill(0xEE);

  let overlap = [
    FixedSegmentRead {
      segment_fp_id: 0,
      offset: 0,
      len: 4096,
    },
    FixedSegmentRead {
      segment_fp_id: 1,
      offset: 4096 - 512,
      len: 4096,
    },
  ];
  assert_eq!(
    store
      .read_segment_fixed_li(&tbl, &overlap, &mut buf)
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );

  // 单项超段长：与单段读同规拒绝。
  let oversized = [FixedSegmentRead {
    segment_fp_id: 0,
    offset: 0,
    len: 4096 + 512,
  }];
  assert_eq!(
    store
      .read_segment_fixed_li(&tbl, &oversized, &mut buf)
      .await
      .unwrap_err(),
    KvError::InvalidArgs
  );

  // 两项计划均未提交，缓冲保持原样。
  assert!(buf.as_slice().iter().all(|byte| *byte == 0xEE));
}
