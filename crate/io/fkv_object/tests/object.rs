use std::time::{Duration, Instant};

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

/// sem-41：`last_modified` 对齐 C++ `CloudObjectInfo::mod_time`。Mem 后端不
/// 记录修改时间，头读 / 列举都必须是 `None`（不能伪造时间）。
#[test]
fn mem_backend_reports_no_last_modified() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    store.put("p/a", payload(b"v"), None, false).await.unwrap();

    assert_eq!(
      store.head("p/a").await.unwrap().unwrap().last_modified,
      None,
      "Mem 没有修改时间，head 必须为 None"
    );
    assert!(
      store
        .list("p/")
        .await
        .unwrap()
        .iter()
        .all(|meta| meta.last_modified.is_none()),
      "递归列举同样是 None"
    );
    assert!(
      store
        .list_level_page("p/", None, 0)
        .await
        .unwrap()
        .meta_li
        .iter()
        .all(|meta| meta.last_modified.is_none()),
      "非递归列举同样是 None（含 common prefix 合成条目）"
    );
  });
}

/// sem-35：非递归列举（C++ `ListObjectRequest` 默认 `recursive=false`）只返回
/// 当前层对象 + common prefixes，等价 S3 ListObjectsV2 `delimiter=/`；递归
/// 列举仍返回全部后代。
#[test]
fn list_level_page_returns_current_level_and_common_prefixes() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    for key in [
      "table.1/a",
      "table.1/b",
      "table.10/c",
      "table.2",
      "tablex/not-in-prefix",
    ] {
      store.put(key, payload(b"v"), None, false).await.unwrap();
    }

    let page = store.list_level_page("table.", None, 0).await.unwrap();
    let key_li: Vec<&str> = page.meta_li.iter().map(|meta| meta.key.as_str()).collect();
    assert_eq!(key_li, ["table.1/", "table.10/", "table.2"]);
    assert_eq!(page.next_token, None);
    // common prefix 是合成条目：没有真实对象的 etag / size。
    assert!(page.meta_li[0].etag.is_empty());
    assert_eq!(page.meta_li[0].size, 0);
    assert_eq!(page.meta_li[2].size, 1, "当前层对象保留真实元数据");

    let all = store.list_page("table.", None, 0).await.unwrap();
    assert_eq!(all.meta_li.len(), 4, "递归列举仍返回全部后代");
  });
}

/// sem-35：非递归分页按「条目」计数；token 可能是 common prefix，下一页必须
/// 跳过它及其全部后代，不重不漏。
#[test]
fn list_level_page_paginates_by_level_entries() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    for key in ["p/a/1", "p/a/2", "p/a/3", "p/b", "p/c/1", "p/d"] {
      store.put(key, payload(b"v"), None, false).await.unwrap();
    }

    let first = store.list_level_page("p/", None, 2).await.unwrap();
    let key_li: Vec<&str> = first.meta_li.iter().map(|meta| meta.key.as_str()).collect();
    assert_eq!(key_li, ["p/a/", "p/b"]);
    assert_eq!(first.next_token.as_deref(), Some("p/b"));

    let second = store
      .list_level_page("p/", first.next_token.as_deref(), 2)
      .await
      .unwrap();
    let key_li: Vec<&str> = second
      .meta_li
      .iter()
      .map(|meta| meta.key.as_str())
      .collect();
    assert_eq!(key_li, ["p/c/", "p/d"]);
    assert_eq!(second.next_token, None, "恰好取满但无后续，必须为 None");

    // token 落在 common prefix 上：下一页跳过该目录的全部后代。
    let page = store.list_level_page("p/", Some("p/a/"), 0).await.unwrap();
    let key_li: Vec<&str> = page.meta_li.iter().map(|meta| meta.key.as_str()).collect();
    assert_eq!(key_li, ["p/b", "p/c/", "p/d"]);
    assert_eq!(page.next_token, None);
  });
}

/// 空 prefix 的非递归列举 = 整个 bucket 的当前层；分页同样按条目计数。
#[test]
fn list_level_page_empty_prefix_lists_bucket_root() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    for key in ["a/1", "a/2", "b", "c/1"] {
      store.put(key, payload(b"v"), None, false).await.unwrap();
    }

    let page = store.list_level_page("", None, 0).await.unwrap();
    let key_li: Vec<&str> = page.meta_li.iter().map(|meta| meta.key.as_str()).collect();
    assert_eq!(key_li, ["a/", "b", "c/"]);
    assert_eq!(page.next_token, None);

    let first = store.list_level_page("", None, 2).await.unwrap();
    let key_li: Vec<&str> = first.meta_li.iter().map(|meta| meta.key.as_str()).collect();
    assert_eq!(key_li, ["a/", "b"]);
    assert_eq!(first.next_token.as_deref(), Some("b"));

    let second = store
      .list_level_page("", first.next_token.as_deref(), 2)
      .await
      .unwrap();
    let key_li: Vec<&str> = second
      .meta_li
      .iter()
      .map(|meta| meta.key.as_str())
      .collect();
    assert_eq!(key_li, ["c/"]);
    assert_eq!(second.next_token, None);
  });
}

/// 目录标记对象（key 恰为 common prefix）与后代并存时按 S3 rollup 处理：
/// 只返回 common prefix 合成条目，不把该对象重复算成当前层对象。
#[test]
fn list_level_page_common_prefix_wins_over_directory_marker() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    for key in ["p/a/", "p/a/x"] {
      store.put(key, payload(b"v"), None, false).await.unwrap();
    }

    let page = store.list_level_page("p/", None, 0).await.unwrap();
    let key_li: Vec<&str> = page.meta_li.iter().map(|meta| meta.key.as_str()).collect();
    assert_eq!(key_li, ["p/a/"]);
    assert!(page.meta_li[0].etag.is_empty());
    assert_eq!(page.meta_li[0].size, 0);
  });
}

/// perf-20：10 万对象、100 个当前层目录。非递归列举只做一次全量扫描，
/// 不再按 1024 条分块重复物化整个 prefix（回归前 98 次 / 980 万条）。
#[test]
fn list_level_page_scans_large_prefix_once() {
  pollster::block_on(async {
    let store = MemObjectStore::new();
    for dir in 0..100 {
      for index in 0..1000 {
        store
          .put(
            &format!("p/d{dir:04}/{index:05}"),
            payload(b"v"),
            None,
            false,
          )
          .await
          .unwrap();
      }
    }
    let before = store.level_scan_keys();
    let start = Instant::now();
    let page = store.list_level_page("p/", None, 0).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(page.meta_li.len(), 100, "只返回 100 个当前层 common prefix");
    assert_eq!(page.meta_li[0].key, "p/d0000/");
    assert_eq!(page.meta_li[99].key, "p/d0099/");
    assert!(page.meta_li.iter().all(|meta| meta.etag.is_empty()));
    assert!(page.meta_li.iter().all(|meta| meta.size == 0));
    assert_eq!(page.next_token, None);
    assert_eq!(
      store.level_scan_keys() - before,
      100_000,
      "一次扫描应只处理 prefix 下每个对象一次，而不是分块数 × 对象数"
    );
    assert!(
      elapsed < Duration::from_secs(2),
      "10 万对象单次归并应在量级内完成，实际 {elapsed:?}"
    );
  });
}
