//! `ttl-write` lane：TTL 回收阶段计量。

use std::time::Instant;

use fkv_io_compio::CompioStore;
use fkv_store::{CLEAN_EXPIRED_MAX_ENTRIES, Store};
use fkv_types::{TableIdent, now_ms};

use crate::{
  hist::{LatencyHist, push_latency},
  lane::PartState,
  workload::{WriteMix, ttl_live_key_num},
};

/// `ttl-write` lane 的默认 TTL 时长（毫秒）。
///
/// 取值远大于一次 bench 的写入阶段，保证写入过程中不出现
/// `expire_ts <= now`（那会走「已过期不落主树」分支，测不到双树写）。
/// 回收阶段把注入时钟推到该到期时刻调用 `clean_expired`，不真实等待。
pub(crate) const DEFAULT_TTL_MS: u64 = 3_600_000;
/// `ttl-write` lane 的回收阶段：把注入时钟推到统一 TTL 到期时刻，逐分区循环
/// `clean_expired` 直到 TTL 树清空。
///
/// `clean_expired` 单轮最多处理 [`CLEAN_EXPIRED_MAX_ENTRIES`] 条，未清完时返回
/// `next_expire_ts == now`，因此要按分区循环到不再有可删条目。清理量与写阶段
/// 的 TTL 位置数做断言：本 lane 所有 TTL 条目共用一个 `expire_ts`，TTL key 由
/// `(expire_ts, user_key)` 唯一确定，条数可由 [`ttl_live_key_num`] 闭式推导。
pub(crate) async fn clean_ttl_lane(
  store_li: &mut [(TableIdent, Store<CompioStore>)],
  part_state_li: &[PartState],
  mix: WriteMix,
) -> Vec<(String, f64, &'static str)> {
  let reclaim_now = mix.ttl_expire_ts;
  let mut deleted = 0u64;
  let mut ttl_deleted = 0u64;
  let mut call_num = 0u64;
  let mut hist = LatencyHist::new();
  let started = Instant::now();
  // 写入阶段若长于 TTL 时长，部分条目在写时就被当作过期丢弃（不落主树、
  // 不建 TTL 索引），回收量与闭式预期不再一一对应；此时只输出指标不硬断言。
  let exact = now_ms() <= reclaim_now;
  for ((tbl, store), state) in store_li.iter_mut().zip(part_state_li) {
    let expected = ttl_live_key_num(state.written, state.count, mix) as u64;
    // 每轮最多 256 条；多留一轮给探测 / 坏 key 的收尾。
    let max_call = expected.div_ceil(CLEAN_EXPIRED_MAX_ENTRIES as u64) + 1;
    let mut part_ttl_deleted = 0u64;
    let mut calls = 0u64;
    loop {
      let req_start = Instant::now();
      let stat = store.clean_expired(tbl, reclaim_now).await.unwrap();
      hist.record(req_start.elapsed());
      call_num += 1;
      calls += 1;
      deleted += stat.deleted as u64;
      ttl_deleted += stat.ttl_deleted as u64;
      part_ttl_deleted += stat.ttl_deleted as u64;
      if stat.ttl_deleted == 0
        || stat.next_expire_ts == 0
        || stat.next_expire_ts > reclaim_now
        || calls >= max_call
      {
        break;
      }
    }
    if exact {
      assert_eq!(
        part_ttl_deleted, expected,
        "TTL 回收条数与写入位置不一致（part {tbl}）"
      );
    }
  }
  let elapsed = started.elapsed().as_secs_f64();
  let mut result_li = vec![
    ("ttl_clean_deleted".to_owned(), deleted as f64, "kv"),
    ("ttl_clean_ttl_deleted".to_owned(), ttl_deleted as f64, "kv"),
    ("ttl_clean_call_num".to_owned(), call_num as f64, "call"),
    (
      "ttl_clean_kv_s".to_owned(),
      deleted as f64 / elapsed,
      "kv/s",
    ),
  ];
  push_latency(&mut result_li, "ttl_clean", &hist);
  result_li
}
