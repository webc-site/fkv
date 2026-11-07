//! 写入轨道、分区映射与写阶段计量。

use std::{
  mem::size_of,
  sync::atomic::{AtomicU64, AtomicUsize, Ordering},
  time::{Duration, Instant},
};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::TableIdent;
use futures::future::join_all;

use crate::{
  hist::{LatencyHist, WindowRecorder},
  rng::CppKeyGen,
  workload::{RngMode, WriteMix, delete_target, is_deleted},
};

/// key 编码尾部的 uint64 宽度（大端）。历史 `simple_bench` 口径 key 就
/// 恰好是这 8 字节；redb 口径的 24B key 在这 8 字节前补零。
pub(crate) const KEY_BYTES: usize = size_of::<u64>();
/// 允许的最大 key 宽度（24B redb 档位的上界，再宽没有对应 benchmark 需求）。
pub(crate) const MAX_KEY_BYTES: usize = 64;

/// 进程级 key 宽度，由 `--key-size` 在启动时设置一次（默认 8，保持历史口径）。
/// benchmark 是单进程工具，读路径每请求都要用它编码，放进原子量避免把宽度
/// 穿过十几层调用签名；运行期不再改变。
static KEY_WIDTH: AtomicUsize = AtomicUsize::new(KEY_BYTES);

/// 设置 key 宽度；仅允许 `[KEY_BYTES, MAX_KEY_BYTES]`，由 CLI 校验后调用。
pub(crate) fn set_key_width(width: usize) {
  debug_assert!((KEY_BYTES..=MAX_KEY_BYTES).contains(&width));
  KEY_WIDTH.store(width, Ordering::Relaxed);
}

/// 分区 `part` 的写状态：key 数与累计写入条数（覆盖写后按 `written % count` 回绕）。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PartState {
  /// 分区 key 数（序列位上界）。
  pub(crate) count: usize,
  /// 该分区累计写入条数（可大于 `count`）。
  pub(crate) written: usize,
  /// `written` 是否来自本进程真实写入。跨进程 `--read-only-existing` 读不到
  /// 写状态时置 `false`，读 / scan 只做长度校验、不做存活断言。
  pub(crate) exact: bool,
}

/// 分区 `part` 写入的 key 下标：分区按 `index % partitions` 分片，序列位是分区内下标。
pub(crate) fn shard_index(part: usize, step: usize, pos: usize) -> usize {
  part + step * pos
}

/// 第 `pos` 个序列位的写请求：按 [`WriteMix`] 决定稀疏间隔与 upsert/delete。
///
/// delete 目标按**分区内序列位**回退 `del_ratio - 1`，保证 tombstone 落在同一分区的
/// 表上（旧实现按全局下标回退，`partitions > 1` 时会跨分区删不存在的 key）。
pub(crate) fn write_entry(
  part: usize,
  step: usize,
  pos: usize,
  mix: WriteMix,
  value: &[u8],
) -> BatchEntry {
  if is_deleted(pos, mix.del_ratio) {
    let target = delete_target(pos, mix.del_ratio);
    return BatchEntry::delete(
      key_bytes(shard_index(part, step, target) * mix.key_interval).to_vec(),
      1,
    );
  }
  let mut entry = BatchEntry::upsert(
    key_bytes(shard_index(part, step, pos) * mix.key_interval).to_vec(),
    value.to_vec(),
    1,
  );
  // TTL lane：按序列位选中一批 upsert 附带 expire_ts，走主树 + TTL 树双写；
  // delete 位不带 TTL（删除本身已清旧 TTL 索引）。
  if mix.ttl_ratio > 0 && mix.ttl_expire_ts != 0 && pos % mix.ttl_ratio == mix.ttl_ratio - 1 {
    entry.expire_ts = mix.ttl_expire_ts;
  }
  entry
}

/// 当前 key 宽度（字节）。
pub(crate) fn key_width() -> usize {
  KEY_WIDTH.load(Ordering::Relaxed)
}

/// key 编码：宽度为 [`key_width`] 字节，**末尾 8 字节**是大端 uint64 下标，
/// 前面补零。宽度为 8 时与历史 `simple_bench` 逐字节一致；宽度为 24 时
/// 字典序仍与下标序一致（C++ FFI 用同一编码，见 `cpp/bench_ffi`）。
pub(crate) fn key_bytes(index: usize) -> Vec<u8> {
  encode_key(index, key_width())
}

/// 按显式宽度编码 key（纯函数，测试用；运行期走 [`key_bytes`] 的进程级宽度）。
pub(crate) fn encode_key(index: usize, width: usize) -> Vec<u8> {
  debug_assert!(width >= KEY_BYTES);
  let mut key = vec![0u8; width];
  key[width - KEY_BYTES..].copy_from_slice(&(index as u64).to_be_bytes());
  key
}

/// 读 / scan 的探测 key：`--rng cpp` 使用原始 key 下标（`CppKeyGen` 写在
/// `[0, rows)`），只有 `--rng grid` 才乘 `key_interval`。把这一步集中在这里，
/// 避免读路径误用 grid 的稀疏步长，查到从未写过的 key。
pub(crate) fn probe_key_bytes(index: usize, mix: WriteMix) -> Vec<u8> {
  if mix.rng.is_grid() {
    key_bytes(index * mix.key_interval)
  } else {
    key_bytes(index)
  }
}

/// 分区 `part` 分到的 key 数：`[0, rows)` 中 `index % partitions == part` 的个数。
///
/// 读链按分区拆分读次数时用它做采样上界，保证各分区总读次数仍等于 `rows`。
pub(crate) fn partition_key_num(rows: usize, partitions: usize, part: usize) -> usize {
  let full = rows / partitions;
  let rem = rows % partitions;
  full + usize::from(part < rem)
}
/// 一条写流水：绑定一个分区，在分区自有 key 序列上推进游标。
///
/// 分区是全局键空间的分片（`index % partitions`），写游标只在分片内推进；
/// 覆盖写模式下越过分片末尾回绕到 0，对齐 C++ `writing_key_ > max_key` 归零。
///
/// `key_gen` 为 `Some` 时走 C++ 兼容随机游走（`--rng cpp`）：每条请求由
/// [`CppKeyGen`] 决定 key 与 delete，`pos` 不再参与生成。
#[derive(Debug, Clone, Copy)]
pub(crate) struct WriteLane {
  /// 分区下标（`store_li` 下标）。
  pub(crate) part: usize,
  /// 分区内 key 的全局步长（＝分区数）。
  pub(crate) step: usize,
  /// 该分区的 key 数，也是游标回绕上界。
  pub(crate) count: usize,
  /// 下一个要写的分区内序列位。
  pub(crate) pos: usize,
  /// C++ 兼容 key / delete 生成器；`None`＝网格口径。
  pub(crate) key_gen: Option<CppKeyGen>,
}

impl WriteLane {
  /// 取本分区接下来 `len` 条写请求；越过序列末尾按 C++ 回绕。
  pub(crate) fn next_batch(&mut self, len: usize, mix: WriteMix, value: &[u8]) -> Vec<BatchEntry> {
    let mut batch_li = Vec::with_capacity(len);
    if let Some(key_gen) = &mut self.key_gen {
      // C++ `Writer` 的批次边界决定「先编码」还是「先步进」，以及回绕检查时机。
      key_gen.start_batch();
      for _ in 0..len {
        let (key, deleted) = key_gen.next_entry();
        let key = key_bytes(key as usize).to_vec();
        batch_li.push(if deleted {
          BatchEntry::delete(key, 1)
        } else {
          BatchEntry::upsert(key, value.to_vec(), 1)
        });
      }
      key_gen.finish_batch();
      return batch_li;
    }
    for _ in 0..len {
      batch_li.push(write_entry(self.part, self.step, self.pos, mix, value));
      self.pos += 1;
      if self.pos == self.count {
        self.pos = 0;
      }
    }
    batch_li
  }

  /// 取本分区接下来 `len` 个纯 delete 请求，key 序列与 [`WriteLane::next_batch`]
  /// 的网格口径一致（分区内顺序位 + 覆盖写回绕），不生成任何 upsert，也不走
  /// C++ 随机游走生成器。
  ///
  /// `delete-only` 的计量阶段用它：先按未计量的 upsert 序列铺满数据集，
  /// 再逐 key 删除，删除 key 与准备阶段写入的 key 一一对应。
  pub(crate) fn next_delete_batch(&mut self, len: usize, mix: WriteMix) -> Vec<BatchEntry> {
    let mut batch_li = Vec::with_capacity(len);
    for _ in 0..len {
      batch_li.push(BatchEntry::delete(
        key_bytes(shard_index(self.part, self.step, self.pos) * mix.key_interval).to_vec(),
        1,
      ));
      self.pos += 1;
      if self.pos == self.count {
        self.pos = 0;
      }
    }
    batch_li
  }
}

/// 写阶段驱动口径。
#[derive(Debug, Clone, Copy)]
pub(crate) enum WriteMode {
  /// 历史口径：每个分区把自有 `count` 个 key 顺序写一遍（每批最多 `batch_size` 条）。
  Once { batch_size: usize },
  /// 对齐 C++：全系统共 `batch_num` 个请求，每请求恰好 `batch_size` 条，越界回绕。
  Churn { batch_size: usize, batch_num: u64 },
  /// `delete-only` 的计量阶段：只发 delete，全系统共享 `batch_num` 批预算；
  /// 每个分区按自有 `count` 顺序删除、越界停止（与 `Once` 同形）。
  DeleteAll { batch_size: usize, batch_num: u64 },
  /// `--warmup-secs`：只把 store 写热，持续到窗口耗尽；指标由调用方丢弃。
  Warmup { batch_size: usize, window: Duration },
}

/// 构造每分区的写流水，下标与 `store_li` 一一对应。
///
/// `rows < partitions` 的空分区也保留流水（`count == 0`），runner 直接跳过，
/// 便于用 `zip` 把流水与各自 store 配对。
///
/// `--rng cpp` 时每个分区各持一份 `mt19937(0)`、key 空间上界 `rows`——C++
/// `simple_bench` 的每个分区是独立数据库、各自一份全量键空间。
pub(crate) fn write_lane_li(rows: usize, partitions: usize, mix: WriteMix) -> Vec<WriteLane> {
  (0..partitions)
    .map(|part| WriteLane {
      part,
      step: partitions,
      count: partition_key_num(rows, partitions, part),
      pos: 0,
      key_gen: (mix.rng == RngMode::Cpp).then(|| CppKeyGen::new(rows)),
    })
    .collect()
}

/// 写完一遍时每分区的批数：`ceil(count / batch_size)`。
pub(crate) fn once_batch_num(count: usize, batch_size: usize) -> usize {
  count.div_ceil(batch_size)
}

/// `delete-only` 删除全部 key 所需的全系统批数：各分区按 `Once` 口径累加。
///
/// 显式 `--write-batchs` 由调用方覆盖；缺省时用这个值作为全局预算，
/// 保证每个分区的 key 恰好被删除一次。
pub(crate) fn delete_batch_num(rows: usize, partitions: usize, batch_size: usize) -> u64 {
  (0..partitions)
    .map(|part| once_batch_num(partition_key_num(rows, partitions, part), batch_size) as u64)
    .sum()
}

/// 一条写流水的产出：直方图 + 批数 + 条数（条数即该分区的写状态，供存活断言）。
pub(crate) struct WriteLaneResult {
  pub(crate) hist: LatencyHist,
  pub(crate) window: WindowRecorder,
  pub(crate) batch_num: u64,
  pub(crate) kv_num: u64,
}

/// 从全系统预算里认领一个 batch；返回 `false` 表示预算已用尽。
///
/// `write_batchs` 是**全系统**总批数（C++ `WriteLoop` 的完成计数跨全部 writer），
/// 各分区流水按完成顺序动态认领，不预先均分。
pub(crate) fn claim_batch(budget: &AtomicU64, batch_num: u64) -> bool {
  budget.fetch_add(1, Ordering::Relaxed) < batch_num
}

/// 跑一条写流水：完成即取本分区下一批，不与其它分区做轮间同步。
///
/// `Once` 模式写满本分区全部 key；`Churn` 模式每请求 `batch_size` 条、从共享预算
/// 认领批数，写满预算即停（对齐 C++ `WriteLoop` 的 `finished` 队列收割）。
pub(crate) async fn run_write_lane(
  store: &mut Store<CompioStore>,
  tbl: &TableIdent,
  lane: WriteLane,
  mode: WriteMode,
  mix: WriteMix,
  value: &[u8],
  budget: &AtomicU64,
) -> WriteLaneResult {
  let mut lane = lane;
  let mut result = WriteLaneResult {
    hist: LatencyHist::new(),
    window: WindowRecorder::new(),
    batch_num: 0,
    kv_num: 0,
  };
  if lane.count == 0 {
    return result;
  }
  let lane_started = Instant::now();
  let mut written = 0usize;
  loop {
    let len = match mode {
      WriteMode::Once { batch_size } => {
        let remaining = lane.count - written;
        if remaining == 0 {
          break;
        }
        remaining.min(batch_size)
      }
      WriteMode::DeleteAll {
        batch_size,
        batch_num,
      } => {
        let remaining = lane.count - written;
        if remaining == 0 || !claim_batch(budget, batch_num) {
          break;
        }
        remaining.min(batch_size)
      }
      WriteMode::Churn {
        batch_size,
        batch_num,
      } => {
        if !claim_batch(budget, batch_num) {
          break;
        }
        batch_size
      }
      WriteMode::Warmup { batch_size, window } => {
        if lane_started.elapsed() >= window {
          break;
        }
        batch_size
      }
    };
    let batch = if matches!(mode, WriteMode::DeleteAll { .. }) {
      lane.next_delete_batch(len, mix)
    } else {
      lane.next_batch(len, mix, value)
    };
    // per-request 采样：µs 分位必须用 µs 级单调时钟，禁用 COARSE 时钟。
    let req_start = Instant::now();
    store.put(tbl, &batch).await.unwrap();
    let req_elapsed = req_start.elapsed();
    result.hist.record(req_elapsed);
    result.window.record(req_elapsed, len as u64);
    result.batch_num += 1;
    result.kv_num += len as u64;
    written += len;
  }
  result
}
/// 写阶段产出：合并后的直方图、批数 / 条数与各分区写状态。
pub(crate) struct WritePhase {
  pub(crate) hist: LatencyHist,
  pub(crate) window: WindowRecorder,
  pub(crate) batch_num: u64,
  pub(crate) kv_num: u64,
  pub(crate) part_state_li: Vec<PartState>,
  pub(crate) elapsed: f64,
}

/// 跑完整个写阶段：每分区一条流水并发在飞，返回合并后的直方图与写状态。
pub(crate) async fn run_write_phase(
  store_li: &mut [(TableIdent, Store<CompioStore>)],
  rows: usize,
  partitions: usize,
  mode: WriteMode,
  mix: WriteMix,
  value: &[u8],
) -> WritePhase {
  let lane_li = write_lane_li(rows, partitions, mix);
  // 覆盖写模式的全系统批数预算；各分区完成即认领，不预先均分。
  let budget = AtomicU64::new(0);
  // 吞吐窗口：单调 µs 时钟，分辨率不低于其包住的 per-request 采样（见 crate 文档）。
  let started = Instant::now();
  let lane_result_li = join_all(
    store_li
      .iter_mut()
      .zip(&lane_li)
      .map(|((tbl, store), lane)| run_write_lane(store, tbl, *lane, mode, mix, value, &budget)),
  )
  .await;
  let elapsed = started.elapsed().as_secs_f64();
  let mut hist = LatencyHist::new();
  let mut window = WindowRecorder::new();
  let mut batch_num = 0u64;
  let mut kv_num = 0u64;
  let mut part_state_li = vec![PartState::default(); partitions];
  for (lane, result) in lane_li.iter().zip(&lane_result_li) {
    hist.merge(&result.hist);
    window.merge(&result.window);
    batch_num += result.batch_num;
    kv_num += result.kv_num;
    part_state_li[lane.part] = PartState {
      count: lane.count,
      written: result.kv_num as usize,
      exact: true,
    };
  }
  // 写入量口径：`Once` 恰好覆盖 `rows` 个 key；`Churn` 恰好
  // `write_batchs * batch_size` 条（对齐 C++ 的全系统批数）；`Warmup` 只看窗口。
  match mode {
    WriteMode::Warmup { .. } => {}
    WriteMode::Once { batch_size } => {
      assert_eq!(kv_num, rows as u64);
      let expect_batch: u64 = lane_li
        .iter()
        .map(|lane| once_batch_num(lane.count, batch_size))
        .sum::<usize>() as u64;
      assert_eq!(batch_num, expect_batch);
    }
    WriteMode::Churn {
      batch_size,
      batch_num: total_batch,
    } => assert_eq!(kv_num, total_batch * batch_size as u64),
    WriteMode::DeleteAll {
      batch_size,
      batch_num: total_batch,
    } => {
      // 预算是全局的：预算足够时每个分区恰好删完自有 key（`kv_num == rows`）；
      // 预算不足时允许部分删除，但不能超过总 key 数。
      let required: u64 = lane_li
        .iter()
        .map(|lane| once_batch_num(lane.count, batch_size) as u64)
        .sum();
      if total_batch >= required {
        assert_eq!(kv_num, rows as u64);
        assert_eq!(batch_num, required);
      } else {
        assert!(kv_num <= rows as u64);
      }
    }
  }
  WritePhase {
    hist,
    window,
    batch_num,
    kv_num,
    part_state_li,
    elapsed,
  }
}
