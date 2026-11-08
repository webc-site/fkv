//! `FixedBuffer` 租约语义：独占借出与区间借出的在飞判定。

use fkv_io::FixedBuffer;

#[test]
fn share_is_exclusive_until_lease_dropped() {
  let mut buf = FixedBuffer::new(64);
  let first = buf.share().expect("空闲缓冲可借出");
  assert!(buf.share().is_none(), "在飞租约存在时再次独占借出必须拒绝");
  assert!(buf.as_mut_slice().is_none(), "在飞租约存在时禁止改写");

  // 租约释放（CQE 已收割）后缓冲回到可用状态。
  drop(first);
  assert!(buf.as_mut_slice().is_some());
  assert!(buf.share().is_some());
}

#[test]
fn share_disjoint_tracks_every_lease() {
  let mut buf = FixedBuffer::new(64);
  // 区间租约可叠加（批量段写），但在飞期间同样禁止改写。
  let near = buf.share_disjoint();
  let far = buf.share_disjoint();
  assert!(buf.as_mut_slice().is_none());
  drop(near);
  assert!(
    buf.as_mut_slice().is_none(),
    "仍有在飞租约时必须继续拒绝改写"
  );
  drop(far);
  assert!(buf.share().is_some());
}
