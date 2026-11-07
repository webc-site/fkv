//! restart 数组公共逻辑：读 offset、二分定位、区间内线性推进。
//!
//! restart 点处 `shared = 0`、timestamp / page_id 为绝对值，可独立解码，
//! 因此先对 restart key 二分，再从选中的 restart 点顺序推进：
//! 页内比较次数 O(log(restarts) + interval)。

use std::cmp::Ordering;

use fkv_error::{KvError, Result};

/// 支持 restart 二分的页内迭代器。
pub(crate) trait Cursor {
  fn restart_cnt(&self) -> u16;
  /// 页内无 entry。
  fn no_entry(&self) -> bool;
  /// 第 `idx` 个 restart 点的完整 key（页内借用）。
  fn restart_key(&self, idx: u16) -> Result<&[u8]>;
  /// 定位到第 `idx` 个 restart 点之前，下一次 `step` 解码该点 entry。
  fn seek_to_restart(&mut self, idx: u16) -> Result<()>;
  fn step(&mut self) -> Result<bool>;
  fn cur_key(&self) -> &[u8];
  /// 移到页尾并置无效；与线性扫描失败后的状态一致。
  fn invalidate(&mut self);
  /// 不推进状态，比较下一 entry 的 key 与 `target`；已到页尾返回 `None`。
  fn peek_cmp(&self, target: &[u8]) -> Result<Option<Ordering>>;
}

/// restart key `<= target` 的个数（restart key 升序）。
fn upper<C: Cursor>(cur: &C, target: &[u8]) -> Result<u16> {
  let (mut lo, mut hi) = (0u16, cur.restart_cnt());
  while lo < hi {
    let mid = lo + (hi - lo) / 2;
    if cur.restart_key(mid)? <= target {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  Ok(lo)
}

/// 校验 restart key 严格升序（畸形页显式拒绝，避免二分静默错定位）。
pub(crate) fn validate_order<C: Cursor>(cur: &C) -> Result<()> {
  let count = cur.restart_cnt();
  if count < 2 {
    return Ok(());
  }
  let mut prev = cur.restart_key(0)?;
  for index in 1..count {
    let key = cur.restart_key(index)?;
    if key <= prev {
      return Err(KvError::Corrupted);
    }
    prev = key;
  }
  Ok(())
}

/// 第一个 key `>= target` 的 entry。
pub(crate) fn seek<C: Cursor>(cur: &mut C, target: &[u8]) -> Result<bool> {
  if cur.no_entry() {
    cur.invalidate();
    return Ok(false);
  }
  // 最后一个 restart key <= target 之前的 entry 全部 < target。
  let idx = upper(cur, target)?.saturating_sub(1);
  cur.seek_to_restart(idx)?;
  while cur.step()? {
    if cur.cur_key() >= target {
      return Ok(true);
    }
  }
  Ok(false)
}

/// 最后一个 key `<= target` 的 entry，单遍：先窥探下一 key，超出即停，无需回退。
pub(crate) fn seek_floor<C: Cursor>(cur: &mut C, target: &[u8]) -> Result<bool> {
  let idx = if cur.no_entry() {
    None
  } else {
    upper(cur, target)?.checked_sub(1)
  };
  let Some(idx) = idx else {
    cur.invalidate();
    return Ok(false);
  };
  cur.seek_to_restart(idx)?;
  // restart key <= target，首步必然命中。
  if !cur.step()? {
    return Err(KvError::Corrupted);
  }
  while cur.peek_cmp(target)?.is_some_and(Ordering::is_le) {
    cur.step()?;
  }
  Ok(true)
}

/// restart 数组第 `idx` 项（u16 LE）。
pub(crate) fn offset_at(page: &[u8], restart_offset: usize, idx: u16) -> Result<usize> {
  let start = restart_offset + usize::from(idx) * 2;
  let value = fkv_codec::get_fixed16(page.get(start..start + 2).ok_or(KvError::Corrupted)?)?;
  Ok(usize::from(value))
}

/// 校验 restart 数组结构：首项等于页头，其余严格递增，且全部落在
/// `[header, restart_offset)` entry 区内。
///
/// 空页（restart 区紧贴页头）至多含页头一项。region 切分与 data / index
/// 迭代器共用此校验，畸形页统一返回 `Corrupted`，避免二分 / 顺序推进静默错定位。
pub(crate) fn validate_offsets(
  page: &[u8],
  header: usize,
  restart_offset: usize,
  count: u16,
) -> Result<()> {
  if restart_offset < header || restart_offset > page.len() {
    return Err(KvError::Corrupted);
  }
  // 空页：没有 entry，restart 数组最多只有页头 offset 一项。
  if restart_offset == header {
    return if count <= 1 {
      Ok(())
    } else {
      Err(KvError::Corrupted)
    };
  }
  if count == 0 {
    return Err(KvError::Corrupted);
  }
  let mut prev = 0usize;
  for idx in 0..count {
    let offset = offset_at(page, restart_offset, idx)?;
    if (idx == 0 && offset != header) || (idx > 0 && offset <= prev) {
      return Err(KvError::Corrupted);
    }
    if offset >= restart_offset {
      return Err(KvError::Corrupted);
    }
    prev = offset;
  }
  Ok(())
}

/// 比较 `head ++ tail` 与 `target`，零拷贝。
pub(crate) fn cmp_split(head: &[u8], tail: &[u8], target: &[u8]) -> Ordering {
  let (t_head, t_tail) = target.split_at(head.len().min(target.len()));
  head.cmp(t_head).then_with(|| tail.cmp(t_tail))
}

pub(crate) fn common_prefix(a: &[u8], b: &[u8]) -> usize {
  a.iter().zip(b).take_while(|(x, y)| x == y).count()
}
