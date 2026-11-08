//! 跨后端 IO 契约（io-41）：segment 尾读零填充、缺资源错误码、`list_*` 升序去重。
//!
//! 同一组断言在 `fkv_io_mem` / `fkv_io_compio` 的 `tests/contract.rs` 镜像。

use fkv_error::KvError;
use fkv_io::{FileIo, FixedSegmentIo, FixedSegmentRead, PageIo, SegmentIo};
use fkv_io_sim::SimStore;
use fkv_types::TableIdent;

fn store() -> SimStore {
  SimStore::new(8)
    .with_pages_per_file(8, 2)
    .with_segment_config(16, 2)
}

fn tbl() -> TableIdent {
  TableIdent::new("contract", 1).unwrap()
}

/// 段是定长单位：请求超过实际写入长度时读取可用前缀，其余零填充并返回 Ok。
#[test]
fn segment_tail_read_zero_fills() {
  pollster::block_on(async {
    let store = store();
    let tbl = tbl();
    store.write_segment(&tbl, 0, b"abc").await.unwrap();

    let mut full = [0xFFu8; 16];
    store.read_segment(&tbl, 0, &mut full).await.unwrap();
    assert_eq!(&full[..3], b"abc");
    assert!(full[3..].iter().all(|byte| *byte == 0));

    // 请求长度短于段长：仍按可用前缀 + 零填充。
    let mut tail = [0xEEu8; 8];
    store.read_segment(&tbl, 0, &mut tail).await.unwrap();
    assert_eq!(&tail[..3], b"abc");
    assert!(tail[3..].iter().all(|byte| *byte == 0));

    // 空读是 no-op，不要求段存在。
    let mut empty: [u8; 0] = [];
    store.read_segment(&tbl, 99, &mut empty).await.unwrap();

    // 超过段长拒绝。
    assert_eq!(
      store
        .read_segment(&tbl, 0, &mut [0u8; 17])
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
  });
}

/// 段 / 页从未落盘 → `ResourceMissing`；Sim 没有文件长度概念，缺页不区分空洞与 EOF。
#[test]
fn missing_segment_and_page_are_resource_missing() {
  pollster::block_on(async {
    let store = store();
    let tbl = tbl();

    let mut segment = [0u8; 16];
    assert_eq!(
      store.read_segment(&tbl, 3, &mut segment).await.unwrap_err(),
      KvError::ResourceMissing
    );

    let mut page = [0u8; 8];
    assert_eq!(
      store.read_page(&tbl, 0, &mut page).await.unwrap_err(),
      KvError::ResourceMissing
    );

    // 已写页的相邻空洞同样是缺页。
    store.write_page(&tbl, 0, &[1u8; 8]).await.unwrap();
    assert_eq!(
      store.read_page(&tbl, 1, &mut page).await.unwrap_err(),
      KvError::ResourceMissing
    );
  });
}

/// fixed 读与 plain 读同一尾读语义，且不越界改写缓冲区其余区域。
#[test]
fn fixed_segment_tail_read_zero_fills() {
  pollster::block_on(async {
    let store = store();
    let tbl = tbl();
    store.write_segment(&tbl, 1, b"xy").await.unwrap();

    let mut buf = store.register_fixed_buffer(16).unwrap();
    buf.as_mut_slice().expect("无在飞 fixed op").fill(0xAA);
    store
      .read_segment_fixed(&tbl, 1, &mut buf, 4, 8)
      .await
      .unwrap();

    assert_eq!(&buf.as_slice()[4..6], b"xy");
    assert!(buf.as_slice()[6..12].iter().all(|byte| *byte == 0));
    assert!(buf.as_slice()[..4].iter().all(|byte| *byte == 0xAA));
    assert!(buf.as_slice()[12..].iter().all(|byte| *byte == 0xAA));
  });
}

/// 批量 fixed 读：互不重叠区间一次读入同一注册缓冲；重叠 / 越界计划整批拒绝，
/// 且拒绝不触碰缓冲（rev-44 残余计划 1）。
#[test]
fn fixed_segment_batch_read_fills_disjoint_regions() {
  pollster::block_on(async {
    let store = store();
    let tbl = tbl();
    store.write_segment(&tbl, 1, b"xy").await.unwrap();
    store.write_segment(&tbl, 2, b"zw").await.unwrap();

    let mut buf = store.register_fixed_buffer(16).unwrap();
    buf.as_mut_slice().expect("无在飞 fixed op").fill(0xAA);

    // 重叠区间：并发提交前整批拒绝，缓冲保持原样。
    let overlap = [
      FixedSegmentRead {
        segment_fp_id: 1,
        offset: 0,
        len: 8,
      },
      FixedSegmentRead {
        segment_fp_id: 2,
        offset: 4,
        len: 8,
      },
    ];
    assert_eq!(
      store
        .read_segment_fixed_li(&tbl, &overlap, &mut buf)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );

    // 越过缓冲长度：整批拒绝。
    let overflow = [FixedSegmentRead {
      segment_fp_id: 1,
      offset: 12,
      len: 8,
    }];
    assert_eq!(
      store
        .read_segment_fixed_li(&tbl, &overflow, &mut buf)
        .await
        .unwrap_err(),
      KvError::InvalidArgs
    );
    assert!(buf.as_slice().iter().all(|byte| *byte == 0xAA));

    let item_li = [
      FixedSegmentRead {
        segment_fp_id: 1,
        offset: 4,
        len: 8,
      },
      FixedSegmentRead {
        segment_fp_id: 2,
        offset: 14,
        len: 2,
      },
    ];
    store
      .read_segment_fixed_li(&tbl, &item_li, &mut buf)
      .await
      .unwrap();

    let view = buf.as_slice();
    assert_eq!(&view[4..6], b"xy");
    assert!(view[6..12].iter().all(|byte| *byte == 0));
    assert_eq!(&view[14..16], b"zw");
    assert!(view[..4].iter().all(|byte| *byte == 0xAA));
    assert!(view[12..14].iter().all(|byte| *byte == 0xAA));
  });
}

/// `list_*` 一律升序去重，供 GC 遍历 / 确定性测试依赖。
#[test]
fn list_file_ids_are_sorted_and_deduped() {
  pollster::block_on(async {
    let store = store();
    let tbl = tbl();

    for file_id in [7u64, 0, 3, 5, 1, 9] {
      store
        .write_page(&tbl, file_id << 2, &[1u8; 8])
        .await
        .unwrap();
    }
    // 同一 file 的第二个页只算一个 file id。
    store
      .write_page(&tbl, (3u64 << 2) | 1, &[2u8; 8])
      .await
      .unwrap();
    assert_eq!(
      store.list_file_id_li(&tbl).await.unwrap(),
      vec![0, 1, 3, 5, 7, 9]
    );

    for segment_fp_id in [9u64, 0, 5, 2, 1] {
      store
        .write_segment(&tbl, segment_fp_id, b"seg")
        .await
        .unwrap();
    }
    // 6 >> 2 == 1，与 5 同属 file 1。
    store.write_segment(&tbl, 6, b"x").await.unwrap();
    assert_eq!(
      store.list_segment_file_id_li(&tbl).await.unwrap(),
      vec![0, 1, 2]
    );
  });
}
