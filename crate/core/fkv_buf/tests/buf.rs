use fkv_buf::{AlignedBuf, PAGE_ALIGN, PagesPool};

#[test]
fn alignment_and_zeroing() {
  let buf = AlignedBuf::new(PAGE_ALIGN * 2);
  assert!(buf.is_aligned(PAGE_ALIGN));
  assert!(buf.iter().all(|byte| *byte == 0));
  assert_eq!(buf.len(), PAGE_ALIGN * 2);
}

#[test]
fn clone_is_independent() {
  let mut buf = AlignedBuf::new(64);
  buf[0] = 1;
  let copy = buf.clone();
  buf[0] = 2;
  assert_eq!(copy[0], 1);
}

#[test]
fn pool_allocates_page() {
  let pool = PagesPool::new(PAGE_ALIGN);
  let page = pool.allocate();
  assert_eq!(page.len(), PAGE_ALIGN);
  assert!(page.is_aligned(PAGE_ALIGN));
}
