//! term CAS、过期 term 与 term 键测试。

use super::*;

#[test]
fn term_cas() {
  pollster::block_on(async {
    let remote = MemObjectStore::new();
    let store = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    store.upsert_term("main", 1, 1).await.unwrap();
    store.upsert_term("main", 1, 2).await.unwrap();
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(2));
    assert_eq!(
      store.upsert_term("main", 1, 1).await.unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

#[test]
fn store_rejects_expired_term() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let mut first = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    first.claim_term("main", 1, 1).await.unwrap();
    first
      .put_with_term(
        &tbl,
        &[BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
        "main",
        1,
        1,
      )
      .await
      .unwrap();

    let second = Store::new(
      CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        "fkv",
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      ),
      512,
      16,
    );
    second.claim_term("main", 1, 2).await.unwrap();
    assert_eq!(
      first
        .put_with_term(
          &tbl,
          &[BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 2)],
          "main",
          1,
          1,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
  });
}

/// cld-41：term CAS 必须一次读取同时拿到 value 与 etag（对齐 C++ `ReadTermFile`
/// 的单次 GET）。旧实现先 `get` 再 `head`，两个计数都会非零。
#[test]
fn term_cas_reads_value_and_etag_in_one_request() {
  pollster::block_on(async {
    let probe = Arc::new(TermProbe::new(None));
    let store = CloudStore::new(
      MemStore::new(512),
      TermProbeStore {
        inner: MemObjectStore::new(),
        probe: Arc::clone(&probe),
      },
      "fkv",
      18,
    );
    assert_eq!(store.upsert_term("main", 1, 5).await.unwrap(), 5);
    assert_eq!(store.upsert_term("main", 1, 7).await.unwrap(), 7);
    let (get, head, versioned) = probe.counts();
    assert_eq!(head, 0, "term CAS 不应再单独发 HEAD 取 etag");
    assert_eq!(get, 0, "term CAS 不应再单独发 GET 取 value");
    assert_eq!(versioned, 2, "两次 upsert 各一次同源读取");
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(7));
  });
}

/// cld-41：读到旧值后、CAS 之前别的 writer 把 term 推进到 9。
/// 旧实现会拿 HEAD 读到的 E9 去 If-Match，把 9 回退成 7；新实现 CAS 失败后重读，
/// 认到 `current > term` 返回 `ExpiredTerm`，term 停在 9。
#[test]
fn term_cas_does_not_roll_back_advanced_term() {
  pollster::block_on(async {
    let inner = MemObjectStore::new();
    let seed = CloudStore::new(
      MemStore::new(512),
      TermProbeStore {
        inner: inner.clone(),
        probe: Arc::new(TermProbe::new(None)),
      },
      "fkv",
      18,
    );
    seed.upsert_term("main", 1, 5).await.unwrap();

    let store = CloudStore::new(
      MemStore::new(512),
      TermProbeStore {
        inner: inner.clone(),
        probe: Arc::new(TermProbe::new(Some(9))),
      },
      "fkv",
      18,
    );
    assert_eq!(
      store.upsert_term("main", 1, 7).await.unwrap_err(),
      KvError::ExpiredTerm
    );
    assert_eq!(store.read_term("main", 1).await.unwrap(), Some(9));
  });
}

/// cld-41：两个 writer 交替 upsert 更高 term，观察到的 CURRENT_TERM 不得回退。
#[test]
fn concurrent_writers_never_roll_back_term() {
  const ROUNDS: u64 = 200;
  let inner = MemObjectStore::new();
  let observed = Arc::new(AtomicU64::new(0));
  scope(|scope| {
    for offset in 0..2u64 {
      let inner = inner.clone();
      let observed = Arc::clone(&observed);
      scope.spawn(move || {
        pollster::block_on(async {
          let store = CloudStore::new(
            MemStore::new(512),
            inner,
            "fkv",
            fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
          );
          // 只比较本线程前后两次读到的 term：另一线程的读可能发生在本线程读之前、
          // 却在本线程 `fetch_max` 之后更新共享 max，用共享 max 比较会误报回退。
          let mut last = 0u64;
          for round in 0..ROUNDS {
            let term = 2 * round + 1 + offset;
            loop {
              match store.upsert_term("main", 1, term).await {
                Ok(_) | Err(KvError::ExpiredTerm) => break,
                // 重试风暴下允许外层重来，但绝不允许回退。
                Err(KvError::TryAgain) => continue,
                Err(err) => panic!("upsert_term(term={term}) 意外错误: {err:?}"),
              }
            }
            let current = store.read_term("main", 1).await.unwrap().unwrap_or(0);
            assert!(current >= last, "term 回退：{last} -> {current}");
            last = current;
            observed.fetch_max(current, Ordering::SeqCst);
          }
        });
      });
    }
  });
  assert_eq!(observed.load(Ordering::SeqCst), 2 * ROUNDS);
}

/// 包装 `MemObjectStore`：`put` 恒以可重试的 `Busy`（CAS 冲突）失败，读取透传。
/// 用于把 term CAS 的固定次数尝试全部耗尽（模拟持续竞争窗口）。
#[derive(Clone)]
struct ConflictPutStore {
  inner: MemObjectStore,
  put_num: Arc<AtomicUsize>,
}

impl ObjectStore for ConflictPutStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    self.inner.get(key).await
  }

  async fn get_versioned(&self, key: &str) -> Result<Option<(Bytes, String)>> {
    // 与生产一致：value 与 etag 同源一次读取，避免 get + head 的窗口。
    self.inner.get_versioned(key).await
  }

  async fn put(
    &self,
    _key: &str,
    _value: Bytes,
    _if_match: Option<&str>,
    _if_none_match: bool,
  ) -> Result<String> {
    self.put_num.fetch_add(1, Ordering::SeqCst);
    Err(KvError::Busy)
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// sem-80：term CAS 尝试全部耗尽时返回**可重试**的 `TryAgain`（有意差异，见
/// `COMPAT_BASELINE` sem-80；C++ `UpsertTermFile` 同输入回不可重试的
/// `CloudErr`）。用一个 `put` 恒冲突的后端把尝试全部耗尽，钉住收尾码、
/// 可重试分类与尝试次数（`10` 与 C++ `kMaxAttempts` 对齐）。
#[test]
fn term_cas_exhaustion_returns_retryable_try_again() {
  pollster::block_on(async {
    let remote = ConflictPutStore {
      inner: MemObjectStore::new(),
      put_num: Arc::new(AtomicUsize::new(0)),
    };
    // 预置一个更低 term：每次迭代都读到需推进的当前值，走 CAS `put` 分支。
    remote
      .inner
      .put("fkv/CURRENT_TERM_main_1", Bytes::from("1"), None, false)
      .await
      .unwrap();
    let store = CloudStore::new(
      MemStore::new(512),
      remote.clone(),
      "fkv",
      fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
    );
    let err = store.upsert_term("main", 1, 5).await.unwrap_err();
    assert_eq!(err, KvError::TryAgain, "收尾码必须为可重试的 TryAgain");
    assert!(err.is_retryable(), "term CAS 耗尽的收尾码必须可重试");
    assert_eq!(
      remote.put_num.load(Ordering::SeqCst),
      10,
      "应恰好耗尽 10 次尝试（对齐 C++ kMaxAttempts）"
    );
  });
}

/// 空 prefix 时 term 对象 key 不能多出前导 `/`：与 `tbl_prefix` 同规则，
/// 对齐 C++ `ComposeKeyFromRemote`（空组件直接跳过），否则跨实现互看不见 term 文件。
#[test]
fn term_key_has_no_leading_slash_with_empty_prefix() {
  pollster::block_on(async {
    for (prefix, want) in [
      ("", "CURRENT_TERM_main_0"),
      ("pfx", "pfx/CURRENT_TERM_main_0"),
      ("pfx/", "pfx/CURRENT_TERM_main_0"),
    ] {
      let remote = MemObjectStore::new();
      let cloud = CloudStore::new(
        MemStore::new(512),
        remote.clone(),
        prefix,
        fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
      );
      assert_eq!(cloud.upsert_term("main", 0, 7).await.unwrap(), 7);
      let key_li: Vec<String> = remote
        .list("")
        .await
        .unwrap()
        .into_iter()
        .map(|meta| meta.key)
        .collect();
      assert!(
        key_li.iter().any(|key| key == want),
        "prefix={prefix:?} 未见 {want}，实际 {key_li:?}"
      );
      assert!(
        !key_li.iter().any(|key| key.starts_with('/')),
        "prefix={prefix:?} 出现前导斜杠 key：{key_li:?}"
      );
      assert_eq!(cloud.read_term("main", 0).await.unwrap(), Some(7));
    }
  });
}
