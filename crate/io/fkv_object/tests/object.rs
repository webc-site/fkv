use bytes::Bytes;
use fkv_error::KvError;
use fkv_object::{MemObjectStore, ObjectMeta, ObjectStore};

/// 测试用静态负载，避免每处写 `Bytes::from_static`。
fn payload(data: &'static [u8]) -> Bytes {
  Bytes::from_static(data)
}

#[test]
fn put_get_list_delete() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    let etag = store.put("a/x", payload(b"x"), None, false).await.unwrap();
    assert_eq!(store.get("a/x").await.unwrap().as_deref(), Some(&b"x"[..]));
    assert_eq!(store.list("a/").await.unwrap().len(), 1);
    assert!(store.put("a/x", payload(b"y"), None, true).await.is_err());
    let etag2 = store
      .put("a/x", payload(b"z"), Some(&etag), false)
      .await
      .unwrap();
    assert_ne!(etag2, etag);
    assert_eq!(store.get("a/x").await.unwrap().as_deref(), Some(&b"z"[..]));
    store.delete("a/x").await.unwrap();
    assert_eq!(store.get("a/x").await.unwrap(), None);
  });
}

#[test]
fn conditional_existing() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    store.put("k", payload(b"1"), None, false).await.unwrap();
    assert_eq!(
      store.put("k", payload(b"2"), None, true).await.unwrap_err(),
      KvError::AlreadyExists
    );
    assert_eq!(
      store
        .put("k", payload(b"2"), Some("bad"), false)
        .await
        .unwrap_err(),
      KvError::Busy
    );
  });
}

/// io-29：按 token 分页遍历，不重不漏。
#[test]
fn list_page_walks_without_duplicates() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    for index in 0..5 {
      store
        .put(&format!("c/{index:02}"), payload(b"v"), None, false)
        .await
        .unwrap();
    }
    let mut token: Option<String> = None;
    let mut key_li = Vec::new();
    loop {
      let page = store.list_page("c/", token.as_deref(), 2).await.unwrap();
      assert!(page.meta_li.len() <= 2);
      key_li.extend(page.meta_li.into_iter().map(|meta| meta.key));
      match page.next_token {
        Some(next) => token = Some(next),
        None => break,
      }
    }
    assert_eq!(
      key_li,
      (0..5)
        .map(|index| format!("c/{index:02}"))
        .collect::<Vec<_>>()
    );
  });
}

/// cld-42：未覆盖 `put_range` 的后端默认做 RMW 后必须覆盖已存在对象，
/// 不能再用 `if_none_match=true` 自相矛盾。
#[test]
fn default_put_range_overwrites_existing_object() {
  struct PlainStore(MemObjectStore);

  impl ObjectStore for PlainStore {
    async fn get(&self, key: &str) -> Result<Option<Bytes>, fkv_error::KvError> {
      self.0.get(key).await
    }
    async fn put(
      &self,
      key: &str,
      value: Bytes,
      if_match: Option<&str>,
      if_none_match: bool,
    ) -> Result<String, fkv_error::KvError> {
      self.0.put(key, value, if_match, if_none_match).await
    }
    async fn head(&self, key: &str) -> Result<Option<ObjectMeta>, fkv_error::KvError> {
      self.0.head(key).await
    }
    async fn delete(&self, key: &str) -> Result<(), fkv_error::KvError> {
      self.0.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>, fkv_error::KvError> {
      self.0.list(prefix).await
    }
  }

  pollster::block_on(async {
    let store = PlainStore(MemObjectStore::new());
    store
      .put("a/f", payload(b"0123456789"), None, false)
      .await
      .unwrap();
    store.put_range("a/f", 4, b"XY").await.unwrap();
    assert_eq!(
      store.get("a/f").await.unwrap().as_deref(),
      Some(&b"0123XY6789"[..])
    );
  });
}

/// cld-41：value 与 etag 必须来自同一次读取。
#[test]
fn get_versioned_returns_value_and_matching_etag() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    let etag = store
      .put("t/term", payload(b"7"), None, false)
      .await
      .unwrap();
    let (value, read_etag) = store.get_versioned("t/term").await.unwrap().unwrap();
    assert_eq!(value.as_ref(), b"7");
    assert_eq!(read_etag, etag);
  });
}

/// io-29：token 为「严格大于」语义；越过末尾为空页；`max_keys == 0` 不限；
/// 恰好取满一页时 `next_token` 为 `None`。
#[test]
fn list_page_token_and_bounds() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    for index in 0..5 {
      store
        .put(&format!("c/{index:02}"), payload(b"v"), None, false)
        .await
        .unwrap();
    }

    let page = store.list_page("c/", Some("c/04"), 2).await.unwrap();
    assert!(page.meta_li.is_empty());
    assert_eq!(page.next_token, None);

    let page = store.list_page("c/", Some("c/02"), 0).await.unwrap();
    let key_li: Vec<&str> = page.meta_li.iter().map(|meta| meta.key.as_str()).collect();
    assert_eq!(key_li, ["c/03", "c/04"]);
    assert_eq!(page.next_token, None);

    let page = store.list_page("c/", None, 5).await.unwrap();
    assert_eq!(page.meta_li.len(), 5);
    assert_eq!(page.next_token, None, "恰好取满但无后续，必须为 None");

    let page = store.list_page("c/", None, 1).await.unwrap();
    assert_eq!(page.meta_li.len(), 1);
    assert_eq!(page.next_token.as_deref(), Some("c/00"));
  });
}
