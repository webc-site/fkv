use fkv_buf::{AlignedBuf, PagesPool};
use fkv_types::PAGE_ALIGN;

#[test]
fn alignment_and_zeroing() {
  let buf = AlignedBuf::new(PAGE_ALIGN * 2);
  assert!(buf.is_aligned(PAGE_ALIGN));
  assert!(buf.iter().all(|byte| *byte == 0));
  assert_eq!(buf.len(), PAGE_ALIGN * 2);
}

#[test]
fn pool_allocates_page() {
  let pool = PagesPool::new(PAGE_ALIGN);
  let page = pool.allocate();
  assert_eq!(page.len(), PAGE_ALIGN);
  assert!(page.is_aligned(PAGE_ALIGN));
}

#[test]
fn pool_recycles_and_caps() {
  let pool = PagesPool::with_max_cached(PAGE_ALIGN, 1);
  let page = pool.allocate();
  let ptr = page.as_ptr();
  pool.recycle(page);
  assert_eq!(pool.cached(), 1);
  let reused = pool.allocate();
  assert_eq!(reused.as_ptr(), ptr);
  assert_eq!(pool.cached(), 0);

  // 超过上限的归还直接释放，不再缓存。
  let first = pool.allocate();
  let second = pool.allocate();
  pool.recycle(first);
  pool.recycle(second);
  assert_eq!(pool.cached(), 1);
}
