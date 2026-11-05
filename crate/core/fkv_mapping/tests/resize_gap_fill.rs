//! core-29：`MappingTbl` 收缩后再稀疏写入，`[old_len, len)` 必须显式回填 `INVALID`。
//!
//! 这是 `doc/migration.md` §8.1 记录的一处**有意偏离**：C++
//! `MappingTbl::ResizeInternal`（`src/storage/page_mapper.cpp:266-300`）增长时只
//! `fill(InvalidValue)` 新分配的 chunk，已存在 chunk 尾部的 stale 槽保持原值，
//! 会把回滚前残留的 `FilePageId` 序列化出去（对端按活跃页计入 retained）。
//! Rust 侧显式回填 `INVALID`。本测试锁住 Rust 行为，防止「对齐 C++」时静默回归。

use fkv_mapping::{CHUNK_SIZE, MappingTbl};
use fkv_types::{INVALID_VALUE, encode_file_page_id, is_file_page_id};

#[test]
fn shrink_then_sparse_set_fills_gap_with_invalid() {
  let mut tbl = MappingTbl::new();
  // 让尾部槽位带上活跃 FilePageId，模拟回滚前已 flush 的页。
  for index in 0..100u32 {
    tbl.set(index, encode_file_page_id(u64::from(index) + 1));
  }
  assert_eq!(tbl.len(), 100);
  assert!(tbl.iter_value_li().all(is_file_page_id));
  // 判别点：槽位 6 在收缩前是 FilePageId(7)。
  assert_eq!(tbl.get(6).unwrap(), encode_file_page_id(7));

  // abort_batch 回滚：收缩到 5（chunk 保持满长，槽位 5.. 仍是旧 FilePageId）。
  tbl.resize(5, INVALID_VALUE);
  assert_eq!(tbl.len(), 5);

  // 稀疏写回**同一个未释放的 chunk** 内（39 < CHUNK_SIZE）。
  tbl.set(39, encode_file_page_id(999));
  assert_eq!(tbl.len(), 40);

  // 增长必须显式回填：若按 C++ `ResizeInternal` 语义只填新 chunk，
  // 槽位 6 会读回收缩前的 FilePageId(7)，下面两条断言即失败。
  assert_ne!(tbl.get(6).unwrap(), encode_file_page_id(7));
  assert_eq!(tbl.get(6).unwrap(), INVALID_VALUE);

  let value_li: Vec<u64> = tbl.iter_value_li().collect();
  assert_eq!(value_li.len(), 40);
  assert_eq!(value_li[39], encode_file_page_id(999));
  for (index, value) in value_li.iter().enumerate().skip(5).take(34) {
    assert_eq!(
      *value, INVALID_VALUE,
      "收缩后再增长的间隙槽位 {index} 必须回填 INVALID（当前 {value:#x}）"
    );
  }

  // 盘面口径：序列化出来的间隙也必须是 INVALID（oracle 字节差分依赖这一条）。
  let roundtrip = MappingTbl::deserialize(&tbl.serialize()).unwrap();
  assert_eq!(roundtrip.iter_value_li().nth(6), Some(INVALID_VALUE));
  assert_ne!(roundtrip.get(6).unwrap(), encode_file_page_id(7));
  assert_eq!(roundtrip.len(), 40);
}

/// 对照：**释放**了 chunk 的收缩（跨 chunk 边界）两侧本来就一致（新 chunk 被填
/// `INVALID`）。这条是上一条的边界说明，避免把偏离范围误读成「所有间隙」。
#[test]
fn shrink_across_chunk_boundary_then_grow_fills_invalid() {
  let mut tbl = MappingTbl::new();
  for index in 0..(CHUNK_SIZE as u32 * 2) {
    tbl.set(index, encode_file_page_id(u64::from(index) + 1));
  }
  assert_eq!(tbl.len(), CHUNK_SIZE * 2);

  tbl.resize(5, INVALID_VALUE);
  // 越过被释放的 chunk 再长回来：新 chunk 全填 INVALID。
  tbl.set(CHUNK_SIZE as u32 + 3, encode_file_page_id(7));
  let value_li: Vec<u64> = tbl.iter_value_li().collect();
  let gap = &value_li[CHUNK_SIZE + 1..CHUNK_SIZE + 3];
  assert_eq!(gap, [INVALID_VALUE, INVALID_VALUE], "{value_li:?}");
  assert!(value_li[..5].iter().all(|value| *value != INVALID_VALUE));
}
