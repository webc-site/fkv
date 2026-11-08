//! 字节序 comparator 与最短分隔键：对齐 C++ `comparator.cpp` 的固定向量、
//! 性质测试与确定性随机对照。

use core::mem::swap;

use fkv_btree::comparator::find_shortest_separator;

/// C++ `BytewiseComparatorImpl::FindShortestSeparator(string_view, string_view)`
/// 的逐行参考实现（`assert(left < right)` 保留为前置条件）。
fn cpp_separator<'a>(left: &[u8], right: &'a [u8]) -> &'a [u8] {
  assert!(left < right, "left={left:?} right={right:?}");
  let mut diff_idx = 0;
  while diff_idx < left.len() && left[diff_idx] == right[diff_idx] {
    diff_idx += 1;
  }
  assert!(diff_idx < right.len(), "left={left:?} right={right:?}");
  &right[..diff_idx + 1]
}

fn common_prefix_len(left: &[u8], right: &[u8]) -> usize {
  left
    .iter()
    .zip(right)
    .take_while(|(left_byte, right_byte)| left_byte == right_byte)
    .count()
}

/// 固定种子 LCG：不引入额外依赖，重放结果稳定。
struct Lcg(u64);

impl Lcg {
  fn new(seed: u64) -> Self {
    Self(seed)
  }

  fn next_u64(&mut self) -> u64 {
    self.0 = self
      .0
      .wrapping_mul(6_364_136_223_846_793_005)
      .wrapping_add(1_442_695_040_888_963_407);
    self.0
  }

  fn below(&mut self, bound: usize) -> usize {
    (self.next_u64() >> 32) as usize % bound
  }

  /// 键长 0..=8，字母表覆盖 0x00 / 0xff 与常见中间值，便于命中公共前缀与边界。
  fn key(&mut self) -> Vec<u8> {
    const ALPHABET: [u8; 6] = [0x00, 0x01, 0x7f, 0x80, 0xfe, 0xff];
    let len = self.below(9);
    (0..len)
      .map(|_| ALPHABET[self.below(ALPHABET.len())])
      .collect()
  }
}

/// 每个向量按 C++ 代码人工推导：`right[..common_prefix + 1]`。
#[test]
fn separator_matches_cpp_vectors() {
  let case_li: &[(&[u8], &[u8], &[u8])] = &[
    (b"abc", b"abd", b"abd"),
    // left 是 right 前缀：取 right[..left.len() + 1]。
    (b"abc", b"abcd", b"abcd"),
    (b"a", b"b", b"b"),
    // 公共前缀 "key0"，首个不同字节在索引 4。
    (b"key0100", b"key0200", b"key02"),
    (b"abc", b"abef", b"abe"),
    (b"", b"a", b"a"),
    // 0xff 边界：前缀情形仍可加一字节。
    (b"\xff", b"\xff\x00", b"\xff\x00"),
    (b"\x00\xff", b"\x01\x00", b"\x01"),
    (b"\xfe", b"\xff", b"\xff"),
    (b"\x01\xff\xff", b"\x02\x00", b"\x02"),
    (b"\x00", b"\x00\x00\xff", b"\x00\x00"),
  ];
  for &(left, right, expected) in case_li {
    assert_eq!(
      find_shortest_separator(left, right),
      expected,
      "left={left:?} right={right:?}"
    );
  }
}

#[test]
fn separator_properties() {
  let mut rng = Lcg::new(0x1234_5678);
  let mut checked = 0usize;
  while checked < 2_000 {
    let (mut left, mut right) = (rng.key(), rng.key());
    if left == right {
      continue;
    }
    if left > right {
      swap(&mut left, &mut right);
    }
    checked += 1;

    let sep = find_shortest_separator(&left, &right);
    let prefix_len = common_prefix_len(&left, &right);
    assert!(left.as_slice() < sep, "left={left:?} sep={sep:?}");
    assert!(sep <= right.as_slice(), "sep={sep:?} right={right:?}");
    assert!(right.starts_with(sep), "sep={sep:?} right={right:?}");
    // C++ 算法固定取公共前缀后的第一个字节，长度即 prefix_len + 1。
    assert_eq!(sep.len(), prefix_len + 1);
    assert!(sep.len() <= left.len() + 1);
  }
}

#[test]
fn separator_matches_cpp_reference_on_random_pairs() {
  let mut rng = Lcg::new(0xdead_beef);
  let mut checked = 0usize;
  while checked < 2_000 {
    let (mut left, mut right) = (rng.key(), rng.key());
    if left == right {
      continue;
    }
    if left > right {
      swap(&mut left, &mut right);
    }
    checked += 1;
    assert_eq!(
      find_shortest_separator(&left, &right),
      cpp_separator(&left, &right),
      "left={left:?} right={right:?}"
    );
  }
}
