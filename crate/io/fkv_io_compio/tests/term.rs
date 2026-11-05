//! 本地 term 文件：单调 CAS、重开读回、格式。

use std::{
  fs::{read, write},
  thread,
};

use compio::runtime::Runtime;
use fkv_error::KvError;
use fkv_io::TermIo;
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;

#[compio::test]
async fn missing_term_reads_none() {
  // `TempRoot` 析构时递归清理，无需手工 `remove_dir_all`。
  let root = TempRoot::new("term_none");
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  assert_eq!(store.read_term("main", 0).await.unwrap(), None);
}

#[compio::test]
async fn upsert_is_strictly_monotonic() {
  let root = TempRoot::new("term_mono");
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  assert_eq!(store.upsert_term("main", 3, 1).await.unwrap(), 1);
  assert_eq!(store.upsert_term("main", 3, 5).await.unwrap(), 5);
  assert_eq!(store.read_term("main", 3).await.unwrap(), Some(5));
  // 过旧 term 拒绝；相等 term 幂等成功（对齐 C++ UpsertTermFile）。
  assert_eq!(
    store.upsert_term("main", 3, 4).await.unwrap_err(),
    KvError::ExpiredTerm
  );
  assert_eq!(store.upsert_term("main", 3, 5).await.unwrap(), 5);
  assert_eq!(store.read_term("main", 3).await.unwrap(), Some(5));
  // branch / shard 互相独立。
  assert_eq!(store.read_term("main", 4).await.unwrap(), None);
  assert_eq!(store.upsert_term("feature", 3, 2).await.unwrap(), 2);
  assert!(!root.join("CURRENT_TERM_main_3.tmp").exists());
}

#[compio::test]
async fn term_survives_reopen_and_uses_cpp_layout() {
  let root = TempRoot::new("term_reopen");
  {
    let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
    store.upsert_term("main", 7, 42).await.unwrap();
  }
  // 文件名 / 十进制 ASCII 内容格式固定。
  assert_eq!(read(root.join("CURRENT_TERM_main_7")).unwrap(), b"42");
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  assert_eq!(store.read_term("main", 7).await.unwrap(), Some(42));
  assert_eq!(
    store.upsert_term("main", 7, 41).await.unwrap_err(),
    KvError::ExpiredTerm
  );
}

#[compio::test]
async fn external_term_file_is_parsed_or_rejected() {
  let root = TempRoot::new("term_parse");
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2);
  // C++ cloud 读取会裁剪首尾空白。
  write(root.join("CURRENT_TERM_main_1"), b" 9\n").unwrap();
  assert_eq!(store.read_term("main", 1).await.unwrap(), Some(9));
  write(root.join("CURRENT_TERM_main_2"), b"x1").unwrap();
  assert_eq!(
    store.read_term("main", 2).await.unwrap_err(),
    KvError::Corrupted
  );
  write(root.join("CURRENT_TERM_main_3"), b"18446744073709551616").unwrap();
  assert_eq!(
    store.read_term("main", 3).await.unwrap_err(),
    KvError::Corrupted
  );
}

#[test]
fn concurrent_claims_of_same_term_all_succeed() {
  const WRITER_NUM: usize = 8;
  let root = TempRoot::new("term_race");
  // 线程只需目录路径；清理交给外层 `TempRoot` 的 `Drop`（在 join 之后）。
  let dir = root.path().to_path_buf();
  let handle_li: Vec<_> = (0..WRITER_NUM)
    .map(|_| {
      let dir = dir.clone();
      thread::spawn(move || {
        Runtime::new().unwrap().block_on(async move {
          CompioStore::new(dir, 512, 2)
            .upsert_term("main", 0, 1)
            .await
        })
      })
    })
    .collect();
  let win_num = handle_li
    .into_iter()
    .map(|handle| handle.join().unwrap())
    .filter(|result| matches!(result, Ok(term) if *term == 1))
    .count();
  assert_eq!(win_num, WRITER_NUM);
}
