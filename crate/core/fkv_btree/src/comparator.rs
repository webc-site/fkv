//! 默认字节序 comparator，对齐 C++ `Comparator::DefaultComparator` /
//! `BytewiseComparatorImpl`。
//!
//! 与 `fkv_page` 的约定一致：只支持**默认字节序比较**，不支持注入自定义
//! comparator；盘面与 manifest / root meta 也不存储 comparator name。
//! C++ 的 `Name()` 只服务同进程内的 comparator 一致性检查，eloqstore 的
//! manifest 与 root meta 没有该字段（fmt-20）。
//!
//! C++ 的 `FindShortestSeparator(std::string *start, std::string_view limit)`
//! 重载无生产调用点，这里只迁移 `string_view` 重载的语义，并以借用返回值
//! 避免分配。

use core::cmp::Ordering;

/// 三路字节序比较。
///
/// 与 `[u8]` 的 `Ord` 一致：逐字节按无符号值比较，短串是长串前缀时更小。
/// 对齐 C++ `BytewiseComparatorImpl::Compare`（`std::string_view::compare`）。
#[inline]
#[must_use]
pub fn compare(left: &[u8], right: &[u8]) -> Ordering {
  left.cmp(right)
}

/// 返回 `right` 的最短前缀 `sep`，满足 `left < sep <= right`。
///
/// 前置条件：`left < right`（字节序）。实现对齐 C++ `FindShortestSeparator`
/// 的 `string_view` 重载：取 `right` 中第一个与 `left` 不同的字节作结尾；
/// `left` 是 `right` 的前缀时取 `right[..left.len() + 1]`。
///
/// # Panics
///
/// `debug_assert!` 在 debug 构建捕获 `left >= right`；违反前置条件时 release
/// 构建也可能因切片越界 panic。
#[inline]
#[must_use]
pub fn find_shortest_separator<'a>(left: &[u8], right: &'a [u8]) -> &'a [u8] {
  debug_assert!(
    left < right,
    "find_shortest_separator requires left < right: left={left:?} right={right:?}"
  );
  // `left < right` 保证 `right` 不会是 `left` 的严格前缀：
  // 公共前缀耗尽时 `left.len() < right.len()`，`diff + 1` 必在 `right` 内。
  let diff = left
    .iter()
    .zip(right)
    .position(|(left_byte, right_byte)| left_byte != right_byte)
    .unwrap_or(left.len());
  &right[..=diff]
}
