//! Put 组合并：同 key 按 timestamp 折叠、后写优先，全程不复制 key / value。

use std::mem::take;

use fkv_api::Request;
use fkv_btree::BatchEntry;

use crate::sender::Envelope;

/// 把一个分区的连续 Put 合并成一次 Store::put。
///
/// 语义：
/// - 每个请求内部同 key 仍按 timestamp 折叠：timestamp 大者胜、相等时后写胜
///   （与 `prepare_write_batch` / `Store::put` 一致，不是无条件后写覆盖）；
/// - 跨请求同 key 时，timestamp 大的胜；timestamp 相等则后请求胜
///   （与 Store 对旧页的 `entry.timestamp < old.timestamp` 过滤一致）；
/// - 请求 term 必须相同（envelope_li 构造时保证），合并后统一校验。
///
/// 实现：对每条 `Request::Put` `mem::take` 出 `batch_li`，把 entry 连同
/// `(请求序, 批内序)` 一起按所有权排序；每个 key 段内取 timestamp 最大者，
/// 相等时排在后面的（后请求 / 批内后写）胜。幸存者 move 进输出，全程不 clone
/// key / value；输出按 key 升序（与旧 BTreeMap 一致）。合并后 envelope 的
/// `batch_li` 均为空，任务路径只再使用其 reply 通道。
pub(crate) fn merge_put_batch_li(envelope_li: &mut [Envelope]) -> Vec<BatchEntry> {
  let total = envelope_li
    .iter()
    .map(|envelope| match &envelope.req {
      Request::Put { batch_li, .. } => batch_li.len(),
      _ => 0,
    })
    .sum();
  merge_batch_iter(
    total,
    envelope_li
      .iter_mut()
      .enumerate()
      .filter_map(|(req_idx, envelope)| match &mut envelope.req {
        Request::Put { batch_li, .. } => Some((req_idx, batch_li)),
        _ => None,
      }),
  )
}

/// 合并一组已按请求顺序排列的 Put 批；entry 被取走后请求侧 `batch_li` 为空。
///
/// `total` 是各批 entry 总数，用于一次预分配排序表；除排序表与输出容器外
/// 不再分配（旧实现只为幸存者各 clone 一次 key / value）。
fn merge_batch_iter<'a>(
  total: usize,
  batch_li: impl Iterator<Item = (usize, &'a mut Vec<BatchEntry>)>,
) -> Vec<BatchEntry> {
  let mut item_li: Vec<(usize, usize, BatchEntry)> = Vec::with_capacity(total);
  for (req_idx, batch) in batch_li {
    item_li.extend(
      take(batch)
        .into_iter()
        .enumerate()
        .map(|(entry_idx, entry)| (req_idx, entry_idx, entry)),
    );
  }
  // key 升序让输出与旧 BTreeMap 一致；同 key 段内请求序 / 批内序升序，保证
  // 「后写」稳定排在后面。
  item_li.sort_unstable_by(|left, right| {
    left
      .2
      .key
      .cmp(&right.2.key)
      .then_with(|| left.0.cmp(&right.0))
      .then_with(|| left.1.cmp(&right.1))
  });

  // 原地压实：每个 key 段只保留 timestamp 最大者；相等时扫描到更后面的
  // `>=` 会覆盖，于是后请求 / 批内后写胜。loser 直接 drop，不复制。
  let mut write = 0;
  let mut read = 0;
  while read < item_li.len() {
    let key = item_li[read].2.key.as_slice();
    let mut best = read;
    let mut next = read + 1;
    while next < item_li.len() && item_li[next].2.key.as_slice() == key {
      if item_li[next].2.timestamp >= item_li[best].2.timestamp {
        best = next;
      }
      next += 1;
    }
    item_li.swap(write, best);
    write += 1;
    read = next;
  }
  item_li.truncate(write);
  item_li.into_iter().map(|(_, _, entry)| entry).collect()
}

/// 仅测试用：直接合并一组 Put 批，供分配计数 / 语义回归复用同一实现。
///
/// `#[doc(hidden)]` 暴露给集成测试，避免测试构造私有的 `Envelope`。
#[doc(hidden)]
pub fn merge_put_batches_for_test(batch_li: &mut [Vec<BatchEntry>]) -> Vec<BatchEntry> {
  let total = batch_li.iter().map(Vec::len).sum();
  merge_batch_iter(total, batch_li.iter_mut().enumerate())
}
