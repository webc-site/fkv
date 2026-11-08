//! data 页四分页重分配：后页不足 1/4、前页超过 3/4 时按字节中点重新切分。

use fkv_btree::OverflowSlot;
use fkv_error::Result;
use fkv_page::{
  CompressionType, DataPageBuilder, DataPageIter, EntrySpec, content_len,
  entry_size as page_entry_size, page_content_len, restart_num,
};

/// 反序列化出一个 data page 的全部 entry（value 保持页内压缩字节）。
pub(crate) struct RawEntry {
  pub(crate) key: Vec<u8>,
  pub(crate) value: Vec<u8>,
  pub(crate) ptr: bool,
  pub(crate) ts: u64,
  pub(crate) expire: u64,
  pub(crate) comp: CompressionType,
  pub(crate) large: bool,
}

pub(crate) fn collect_entry_li(page: &[u8]) -> Result<Vec<RawEntry>> {
  let mut out = Vec::new();
  let mut iter = DataPageIter::new(page)?;
  while iter.advance()? {
    out.push(RawEntry {
      key: iter.key().to_vec(),
      value: iter.value().to_vec(),
      ptr: iter.is_overflow(),
      ts: iter.timestamp(),
      expire: iter.expire_ts(),
      comp: iter.compression(),
      large: iter.is_large_value(),
    });
  }
  Ok(out)
}

/// 用给定 entry 构建单页；放不下返回 `None`。
pub(crate) fn build_raw_page(
  entry_li: &[RawEntry],
  page_size: usize,
  restart_interval: u16,
) -> Result<Option<Vec<u8>>> {
  let mut builder = DataPageBuilder::new(page_size, restart_interval)?;
  for entry in entry_li {
    if !builder.add(EntrySpec {
      key: &entry.key,
      value: &entry.value,
      ts: entry.ts,
      expire_ts: entry.expire,
      overflow: entry.ptr,
      compression: entry.comp,
      large_value: entry.large,
    })? {
      return Ok(None);
    }
  }
  if builder.is_empty() {
    return Ok(None);
  }
  Ok(Some(builder.finish()?))
}

/// 索引页重分配阈值（页大小四分位）。
pub(crate) const fn rebalance_bounds(page_size: usize) -> (usize, usize) {
  let quarter = page_size / 4;
  (quarter, page_size - quarter)
}

/// 四分页重分配判定：后页不足 1/4、前页超过 3/4 且前页 restart 数 > 1。
///
/// 数据页与索引页共用同一口径，避免两套阈值各自漂移（C++ `FinishDataPage` /
/// `FinishIndexPage` 同为 quarter / three_quarter 判定）。
pub(crate) fn needs_quarter_rebalance(prev: &[u8], cur: &[u8], page_size: usize) -> Result<bool> {
  let (quarter, three_quarter) = rebalance_bounds(page_size);
  Ok(
    usize::from(content_len(cur)?) < quarter
      && usize::from(content_len(prev)?) > three_quarter
      && restart_num(prev)? > 1,
  )
}

/// 对齐 C++ `BatchWriteTask::Redistribute`：当后页不足 1/4、前页超过 3/4
/// 且前页 restart 数 > 1 时，把两页的 entry 重新按字节均分，消除极小尾页。
/// 切分点以 `(prev_len + cur_len) / 2` 字节中点为起点（对齐 C++ `average_len`），
/// 而非条目数中点，避免变长 entry 下切出不均页。
///
/// `overflow_li` 中落在被重分配两页的 slot 按新切分键重定位 `page_idx`；必须早于
/// `write_overflow_value_li` 调用，此时页内仍是等长占位指针，原地迁移不改变编码。
/// 返回与 `page_li` 等长的标记：`true` 表示该页作为重分配新页被替换、首键已变，
/// 其父 index 分隔键需改用真实首键（见 `page_separator_li`）。
pub(crate) fn rebalance_data_page_li(
  page_li: &mut [Vec<u8>],
  page_size: usize,
  restart_interval: u16,
  overflow_li: &mut [OverflowSlot],
  redistributed_li: &mut Vec<bool>,
) -> Result<()> {
  redistributed_li.clear();
  if page_li.len() < 2 {
    return Ok(());
  }
  redistributed_li.resize(page_li.len(), false);
  for idx in 1..page_li.len() {
    if !needs_quarter_rebalance(&page_li[idx - 1], &page_li[idx], page_size)? {
      continue;
    }
    let prev_len = usize::from(content_len(&page_li[idx - 1])?);
    let cur_len = usize::from(content_len(&page_li[idx])?);
    let mut entry_li = collect_entry_li(&page_li[idx - 1])?;
    entry_li.extend(collect_entry_li(&page_li[idx])?);
    let n = entry_li.len();
    if n < 2 {
      continue;
    }
    let target = (prev_len + cur_len) >> 1;
    let Some(k) = pick_split(&entry_li, target, page_size, restart_interval)? else {
      continue;
    };
    // 估算已保证两侧各装得下：这里每页只实际编码一次。
    let Some(first) = build_raw_page(&entry_li[..k], page_size, restart_interval)? else {
      continue;
    };
    let Some(second) = build_raw_page(&entry_li[k..], page_size, restart_interval)? else {
      continue;
    };
    // 落在本对的 overflow slot：按新切分键重定位到所属页（keys 严格升序）。
    let split_key = entry_li[k].key.as_slice();
    for slot in overflow_li.iter_mut() {
      if slot.page_idx == idx - 1 || slot.page_idx == idx {
        slot.page_idx = if slot.key.as_slice() < split_key {
          idx - 1
        } else {
          idx
        };
      }
    }
    page_li[idx - 1] = first;
    page_li[idx] = second;
    // 后页是重分配新生成的新页，首键已变：其分隔键须改用真实首键。
    redistributed_li[idx] = true;
  }
  Ok(())
}

/// 单条 entry 的两种编码长度：页首 / restart 共用「无前驱」口径（共享前缀与
/// 时间戳基点都归零），restart 只多出 restart 数组的 2 字节偏移。
#[derive(Clone, Copy)]
pub(crate) struct EntryLenPair {
  /// 页首 / restart：无共享前缀、时间戳基点为 0。
  pub(crate) first: usize,
  /// 链式：与页内前一条共享前缀。
  pub(crate) chain: usize,
}

/// 按给定前驱角色估算单条 entry 的编码长度。
pub(crate) fn raw_entry_len(
  entry: &RawEntry,
  prev: Option<&RawEntry>,
  restart: bool,
) -> Result<usize> {
  Ok(
    page_entry_size(
      EntrySpec {
        key: &entry.key,
        value: &entry.value,
        ts: entry.ts,
        expire_ts: entry.expire,
        overflow: entry.ptr,
        compression: entry.comp,
        large_value: entry.large,
      },
      prev.map(|prev| (prev.key.as_slice(), prev.ts)),
      restart,
    )?
    .len,
  )
}

/// 以 `(prev_len + cur_len) / 2` 为字节目标，单次线性扫描选出重分配切分点。
///
/// 原实现逐个候选切分点整页重编码前后两页，最坏 O(n²)；这里改用同口径的
/// `entry_size` 做可行性判断：
/// - 前缀页只依赖前 k 条、长度随 k 单调递增，反向不存在，正向一遍即得；
/// - 后缀页 `entry_li[k..]` 的条目长度只与其 restart 对齐有关：把「restart 相对
///   链式的长度差」按 `i % restart_interval` 分组，反向扫描累加即可在 O(n) 内
///   得到所有 k 的后缀页长度。
///
/// 选定后两侧各只编码一次，切分点与逐点试编码的结果完全一致。
pub(crate) fn pick_split(
  entry_li: &[RawEntry],
  target: usize,
  page_size: usize,
  restart_interval: u16,
) -> Result<Option<usize>> {
  let n = entry_li.len();
  if n < 2 {
    return Ok(None);
  }
  let (prefix_li, suffix_li) = split_len_li(entry_li, restart_interval)?;
  // 与原切分点试探的停止条件一致：首页累加到 `target` 即停，至少留一条给后页。
  let mut k0 = 0;
  for (k, &len) in prefix_li.iter().enumerate().take(n).skip(1) {
    if len > page_size {
      break;
    }
    k0 = k;
    if len >= target {
      break;
    }
  }
  let k0 = k0.max(1);
  // 与原探测顺序一致：k0, k0+1, k0-1, k0+2, k0-2 ...
  let mut picked = None;
  for delta in 0..n {
    for k in [k0 + delta, k0.saturating_sub(delta)] {
      if k == 0 || k >= n {
        continue;
      }
      if prefix_li[k] <= page_size && suffix_li[k] <= page_size {
        picked = Some(k);
        break;
      }
    }
    if picked.is_some() || (k0 + delta >= n && k0 <= delta) {
      break;
    }
  }
  Ok(picked)
}

/// 一遍正向 + 一遍反向求出所有切分点的前后页内容长度：
/// `prefix[k] = len(entry_li[..k])`、`suffix[k] = len(entry_li[k..])`。
///
/// 后缀页的条目长度只与 restart 对齐有关：`index % restart_interval` 相同的
/// 条目在给定后缀里同为 restart，故把「restart 相对链式的长度差」按余数分组
/// 反向累加，即可一次得到全部 k。
pub(crate) fn split_len_li(
  entry_li: &[RawEntry],
  restart_interval: u16,
) -> Result<(Vec<usize>, Vec<usize>)> {
  let n = entry_li.len();
  let ri = usize::from(restart_interval).max(1);
  let mut len_li = Vec::with_capacity(n);
  let mut prefix_li = Vec::with_capacity(n + 1);
  prefix_li.push(page_content_len(0, 1));
  for (idx, entry) in entry_li.iter().enumerate() {
    let first = raw_entry_len(entry, None, false)?;
    let lens = EntryLenPair {
      first,
      chain: match idx.checked_sub(1) {
        Some(prev) => raw_entry_len(entry, Some(&entry_li[prev]), false)?,
        None => first,
      },
    };
    let add = if idx == 0 {
      lens.first
    } else if idx % ri == 0 {
      lens.first + 2
    } else {
      lens.chain
    };
    prefix_li.push(prefix_li[idx] + add);
    len_li.push(lens);
  }
  let mut suffix_li = vec![0usize; n];
  let mut residue_li = vec![0i64; ri];
  let mut chain_sum = 0i64;
  for k in (1..n).rev() {
    // 后缀 `entry_li[k..]` 的 restart 数（含页首隐含项）。
    let restart_cnt = 1 + (n - k - 1) / ri;
    let total = page_content_len(0, restart_cnt) as i64
      + len_li[k].first as i64
      + chain_sum
      + residue_li[k % ri];
    suffix_li[k] = usize::try_from(total).unwrap_or(usize::MAX);
    chain_sum += len_li[k].chain as i64;
    residue_li[k % ri] += len_li[k].first as i64 - len_li[k].chain as i64;
  }
  Ok((prefix_li, suffix_li))
}
