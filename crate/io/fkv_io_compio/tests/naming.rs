use std::{
  env::temp_dir,
  fs::{create_dir_all, remove_dir_all, remove_file, write},
  path::PathBuf,
  process::id,
  sync::atomic::{AtomicU64, Ordering},
  thread::sleep,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use fkv_error::KvError;
use fkv_io::{FileIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_types::TableIdent;

fn temp_root() -> PathBuf {
  static NEXT_ID: AtomicU64 = AtomicU64::new(0);
  let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  let root = temp_dir().join(format!(
    "fkv_naming_{}_{}_{}",
    id(),
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos(),
    seq
  ));
  create_dir_all(&root).unwrap();
  root
}

#[compio::test]
async fn data_and_segment_files_use_branch_and_term() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2);

  store.set_file_identity("feature-1", 7);
  store.write_page(&tbl, 0, &vec![1u8; 512]).await.unwrap();
  assert!(root.join("demo.1").join("data_0_feature-1_7").exists());
  store
    .write_segment(&tbl, 0, &vec![2u8; 4096])
    .await
    .unwrap();
  assert!(root.join("demo.1").join("segment_0_feature-1_7").exists());

  // 新 term 创建新文件；旧 term 文件仍可按 file id 读回。
  store.set_file_identity("feature-1", 8);
  store.write_page(&tbl, 4, &vec![3u8; 512]).await.unwrap();
  store
    .write_segment(&tbl, 4, &vec![4u8; 4096])
    .await
    .unwrap();
  assert!(root.join("demo.1").join("data_1_feature-1_8").exists());
  assert!(root.join("demo.1").join("segment_1_feature-1_8").exists());

  let mut page = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 1);
  store.read_page(&tbl, 4, &mut page).await.unwrap();
  assert_eq!(page[0], 3);
  let mut segment = vec![0u8; 4096];
  store.read_segment(&tbl, 0, &mut segment).await.unwrap();
  assert_eq!(segment[0], 2);
  store.read_segment(&tbl, 4, &mut segment).await.unwrap();
  assert_eq!(segment[0], 4);

  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn path_cache_avoids_repeated_directory_scans() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2);
  store.write_page(&tbl, 0, &vec![1u8; 512]).await.unwrap();
  store
    .write_segment(&tbl, 0, &vec![2u8; 4096])
    .await
    .unwrap();
  let (data_scans, segment_scans) = store.path_scan_num();
  let mut page = vec![0u8; 512];
  let mut segment = vec![0u8; 4096];
  for _ in 0..50 {
    store.read_page(&tbl, 0, &mut page).await.unwrap();
    store.read_segment(&tbl, 0, &mut segment).await.unwrap();
  }
  assert_eq!(store.path_scan_num(), (data_scans, segment_scans));

  // identity 变化清空缓存：首次按 file id 重新定位一次，之后命中缓存。
  store.set_file_identity("feature", 8);
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  let (data_scans, segment_scans) = store.path_scan_num();
  assert_eq!(data_scans, 3);
  assert_eq!(segment_scans, 3);
  for _ in 0..20 {
    store.read_page(&tbl, 0, &mut page).await.unwrap();
  }
  assert_eq!(store.path_scan_num().0, data_scans);
  assert_eq!(store.path_scan_num().1, segment_scans);
}

#[compio::test]
async fn remove_invalidates_path_cache() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2);
  store.write_page(&tbl, 0, &vec![1u8; 512]).await.unwrap();
  store.remove_file(&tbl, 0).await.unwrap();
  let mut page = vec![0u8; 512];
  assert_eq!(
    store.read_page(&tbl, 0, &mut page).await.unwrap_err(),
    KvError::ResourceMissing
  );
  store.set_file_identity("feature", 9);
  store.write_page(&tbl, 0, &vec![3u8; 512]).await.unwrap();
  assert!(root.join("demo.1").join("data_0_feature_9").exists());
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 3);
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn preload_indexes_all_files_with_one_scan() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2);
  store.set_file_identity("feature", 3);
  store.write_page(&tbl, 0, &vec![1u8; 512]).await.unwrap();
  store.write_page(&tbl, 4, &vec![2u8; 512]).await.unwrap();
  store
    .write_segment(&tbl, 0, &vec![3u8; 4096])
    .await
    .unwrap();
  store
    .write_segment(&tbl, 4, &vec![4u8; 4096])
    .await
    .unwrap();

  store.invalidate_path_cache();
  assert_eq!(store.preload(&tbl), 4);
  let scans = store.path_scan_num();
  let mut page = vec![0u8; 512];
  let mut segment = vec![0u8; 4096];
  for _ in 0..20 {
    store.read_page(&tbl, 0, &mut page).await.unwrap();
    store.read_page(&tbl, 4, &mut page).await.unwrap();
    store.read_segment(&tbl, 0, &mut segment).await.unwrap();
    store.read_segment(&tbl, 4, &mut segment).await.unwrap();
  }
  assert_eq!(store.path_scan_num(), scans);

  // 预加载后目录新增文件：miss 触发一次重扫，之后继续命中。
  store.write_page(&tbl, 8, &vec![5u8; 512]).await.unwrap();
  store.read_page(&tbl, 8, &mut page).await.unwrap();
  assert_eq!(page[0], 5);
  let scans = store.path_scan_num();
  for _ in 0..10 {
    store.read_page(&tbl, 8, &mut page).await.unwrap();
  }
  assert_eq!(store.path_scan_num(), scans);
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn external_directory_change_is_detected() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2);
  store.write_page(&tbl, 0, &vec![1u8; 512]).await.unwrap();
  let mut page = vec![0u8; 512];
  assert_eq!(
    store.read_page(&tbl, 4, &mut page).await.unwrap_err(),
    KvError::ResourceMissing
  );

  // 模拟 standby / rsync 在进程外落地新文件。
  sleep(Duration::from_millis(10));
  write(root.join("demo.1").join("data_1_main_0"), vec![7u8; 1024]).unwrap();
  store.read_page(&tbl, 4, &mut page).await.unwrap();
  assert_eq!(page[0], 7);
  remove_dir_all(root).unwrap();
}

#[compio::test]
async fn stale_cached_path_is_recovered() {
  let root = temp_root();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.clone(), 512, 2).with_segment_config(4096, 2);
  store.write_page(&tbl, 0, &vec![1u8; 512]).await.unwrap();
  let mut page = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 1);

  // 文件被外部替换成同 file id、不同 branch/term 的名字。
  let dir = root.join("demo.1");
  remove_file(dir.join("data_0_main_0")).unwrap();
  write(dir.join("data_0_feature_9"), vec![9u8; 512]).unwrap();
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 9);
  remove_dir_all(root).unwrap();
}
