use fkv_error::KvError;
use fkv_object::{MemObjectStore, ObjectStore};

#[test]
fn put_get_list_delete() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    let etag = store.put("a/x", b"x", None, false).await.unwrap();
    assert_eq!(store.get("a/x").await.unwrap(), Some(b"x".to_vec()));
    assert_eq!(store.list("a/").await.unwrap().len(), 1);
    assert!(store.put("a/x", b"y", None, true).await.is_err());
    let etag2 = store.put("a/x", b"z", Some(&etag), false).await.unwrap();
    assert_ne!(etag2, etag);
    assert_eq!(store.get("a/x").await.unwrap(), Some(b"z".to_vec()));
    store.delete("a/x").await.unwrap();
    assert_eq!(store.get("a/x").await.unwrap(), None);
  });
}

#[test]
fn conditional_existing() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    store.put("k", b"1", None, false).await.unwrap();
    assert_eq!(
      store.put("k", b"2", None, true).await.unwrap_err(),
      KvError::AlreadyExists
    );
    assert_eq!(
      store.put("k", b"2", Some("bad"), false).await.unwrap_err(),
      KvError::Busy
    );
  });
}
