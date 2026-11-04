use fkv_cache::PageCache;

#[test]
fn insert_pin_evict() {
  let mut cache = PageCache::new(4096, 2);
  let r1 = cache.insert(1, 10);
  let r2 = cache.insert(2, 20);
  assert_eq!(cache.len(), 2);
  assert_eq!(cache.pin(1).unwrap().generation, r1.generation);
  assert!(cache.unpin(r1));
  assert!(cache.unpin(r2));
  assert_eq!(cache.evict_one(), Some((2, 20)));
  assert!(cache.resolve(r2).is_none());
  assert!(cache.resolve(r1).is_some());
  assert_eq!(cache.len(), 1);
}

#[test]
fn pinned_is_not_evicted() {
  let mut cache = PageCache::new(4096, 1);
  let r1 = cache.insert(1, 10);
  assert_eq!(cache.evict_one(), None);
  assert!(cache.unpin(r1));
  assert_eq!(cache.evict_one(), Some((1, 10)));
}

#[test]
fn remove_returns_file_page() {
  let mut cache = PageCache::new(4096, 1);
  let _r1 = cache.insert(1, 10);
  assert_eq!(cache.remove(1), Some(10));
  assert!(cache.is_empty());
}
