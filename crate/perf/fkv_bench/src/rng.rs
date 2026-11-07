//! C++ `std::mt19937` 复刻与 `simple_bench` 兼容的 key / delete 生成器。
//!
//! C++ `simple_bench` 用 `std::mt19937 rand_gen(0)`（libstdc++ 实现）逐条生成写
//! 请求：`writing_key_ += rand_gen() % 4 + 1` 推进 key，紧接着
//! `rand_gen() % 4 == 0` 决定本条是否改成 delete。要让两侧的 key 序列与
//! tombstone 分布逐条一致，必须**逐位复刻** MT19937 的状态机与取模口径，
//! 不能用 Rust 侧 `fastrand` 之类的其它 PRNG。
//!
//! 状态机常量与 `init_genrand` / `genrand_uint32` 对齐 MT19937 参考实现
//! （Matsumoto & Nishimura 2002），`%` 取模在 `u32` 上与 C++ `uint32_t % 4`
//! 同义（`4` 是 2 的幂，libstdc++ 亦走取模，不做 rejection sampling）。

/// 状态数组长度 `N`。
const N: usize = 624;
/// 中间词偏移 `M`。
const M: usize = 397;
/// 扭曲矩阵 `MATRIX_A`。
const MATRIX_A: u32 = 0x9908_b0df;
/// 高位掩码。
const UPPER_MASK: u32 = 0x8000_0000;
/// 低位掩码。
const LOWER_MASK: u32 = 0x7fff_ffff;

/// MT19937 状态机（`std::mt19937` 同款）。
///
/// 624 个 `u32` 状态按值内联；`Copy` 让写流水能按值传递而不额外分配。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mt19937 {
  mt: [u32; N],
  index: usize,
}

impl Mt19937 {
  /// 按 `init_genrand(seed)` 初始化；`seed == 0` 即 C++ `std::mt19937(0)`。
  pub(crate) fn new(seed: u32) -> Self {
    let mut mt = [0u32; N];
    mt[0] = seed;
    for i in 1..N {
      mt[i] = 1_812_433_253u32
        .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
        .wrapping_add(i as u32);
    }
    Self { mt, index: N }
  }

  /// 生成下一批状态（`generate_numbers`）。
  fn twist(&mut self) {
    for i in 0..N {
      let y = (self.mt[i] & UPPER_MASK) | (self.mt[(i + 1) % N] & LOWER_MASK);
      let mag = if y & 1 != 0 { MATRIX_A } else { 0 };
      self.mt[i] = self.mt[(i + M) % N] ^ (y >> 1) ^ mag;
    }
    self.index = 0;
  }

  /// 输出一个 32 位随机数（`genrand_uint32`）。
  pub(crate) fn next_u32(&mut self) -> u32 {
    if self.index >= N {
      self.twist();
    }
    let mut y = self.mt[self.index];
    self.index += 1;
    y ^= y >> 11;
    y ^= (y << 7) & 0x9d2c_5680;
    y ^= (y << 15) & 0xefc6_0000;
    y ^= y >> 18;
    y
  }
}

/// `simple_bench` 写请求生成器：固定 `seed 0`，key 以 1–4 的随机步长前进。
///
/// 批次语义对齐 C++ `Writer`：
/// - **首个 batch** 的每条先编码当前 `writing_key_` 再步进（构造时先填好一批）；
/// - 之后每个 batch 先步进再编码；
/// - 回绕检查 `writing_key_ > max_key → 0` 只在**整批填完后**执行一次，
///   因此批内 key 可以短暂超过 `max_key`（C++ 同款）。
///
/// 每条请求恰好消耗两次 MT19937 输出（步进一次、delete 判定一次），与 C++ 的
/// 调用顺序一致；顺序错了 key 序列会立刻分叉。
#[derive(Debug, Clone, Copy)]
pub(crate) struct CppKeyGen {
  mt: Mt19937,
  /// 当前 key，初值 0（与 C++ `writing_key_ = 0` 对齐）。
  writing_key: u64,
  max_key: u64,
  /// 下一条 `start_batch` 是否仍走「先编码、后步进」的首批语义。
  first_batch: bool,
  /// 正在填充的批次是否为首批（由 [`Self::start_batch`] 固定）。
  batch_first: bool,
}

impl CppKeyGen {
  /// `max_key` 是 key 空间上界（对齐 C++ `max_key`），至少 1。
  pub(crate) fn new(max_key: usize) -> Self {
    Self {
      mt: Mt19937::new(0),
      writing_key: 0,
      max_key: max_key.max(1) as u64,
      first_batch: true,
      batch_first: true,
    }
  }

  /// 开始填充一个 batch：把当前是否首批固定到本批次。
  pub(crate) fn start_batch(&mut self) {
    self.batch_first = self.first_batch;
  }

  /// 结束一个 batch：后续批次改用「先步进、后编码」，并执行整批一次的回绕检查。
  ///
  /// 回绕阈值是 `>` 而不是 `>=`：`writing_key_ == max_key` 时不归零，与 C++ 一致。
  pub(crate) fn finish_batch(&mut self) {
    self.first_batch = false;
    if self.writing_key > self.max_key {
      self.writing_key = 0;
    }
  }

  /// 下一条写请求：返回 `(key 下标, 是否 delete)`。
  ///
  /// 需先用 [`Self::start_batch`] 开始一批、填完后调用 [`Self::finish_batch`]。
  pub(crate) fn next_entry(&mut self) -> (u64, bool) {
    let step = u64::from(self.mt.next_u32() % 4) + 1;
    let key = if self.batch_first {
      // 首批：编码当前 key，再步进到下一个 key。
      let key = self.writing_key;
      self.writing_key = self.writing_key.saturating_add(step);
      key
    } else {
      // 后续批次：先步进，再编码。
      self.writing_key = self.writing_key.saturating_add(step);
      self.writing_key
    };
    let deleted = self.mt.next_u32() % 4 == 0;
    (key, deleted)
  }
}
