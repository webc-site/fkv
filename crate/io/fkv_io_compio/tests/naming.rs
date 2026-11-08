use std::{
  fs::{create_dir_all, read, read_dir, remove_file, write},
  thread::sleep,
  time::Duration,
};

use fkv_error::KvError;
use fkv_io::{FileIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

#[compio::test]
async fn data_and_segment_files_use_branch_and_term() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);

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
}

#[compio::test]
async fn path_cache_avoids_repeated_directory_scans() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
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
  // 写路径按 (branch, term) 确定性生成，不触发目录扫描，故只有这次重扫。
  store.set_file_identity("feature", 8);
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  let (data_scans, segment_scans) = store.path_scan_num();
  assert_eq!(data_scans, 1);
  assert_eq!(segment_scans, 1);
  for _ in 0..20 {
    store.read_page(&tbl, 0, &mut page).await.unwrap();
  }
  assert_eq!(store.path_scan_num().0, data_scans);
  assert_eq!(store.path_scan_num().1, segment_scans);
}

#[compio::test]
async fn remove_invalidates_path_cache() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
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
}

#[compio::test]
async fn preload_indexes_all_files_with_one_scan() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
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
}

#[compio::test]
async fn external_directory_change_is_detected() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
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
}

#[compio::test]
async fn stale_cached_path_is_recovered() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
  store.write_page(&tbl, 0, &vec![1u8; 512]).await.unwrap();
  let mut page = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 1);

  // 文件被外部替换成同 file id、不同 branch/term 的名字。
  let dir = root.join("demo.1");
  remove_file(dir.join("data_0_main_0")).unwrap();
  write(dir.join("data_0_feature_9"), vec![9u8; 512]).unwrap();
  // 外部替换文件必须由替换方显式失效 FD 缓存（对齐 C++ `CloseTableFiles`）；
  // 命中路径不再做 statx（io-01）。
  store.invalidate_path_cache();
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 9);
}

/// io-04：目录里同时存在旧 term 与新 term 的同 file id 文件时，
/// 写入必须命中当前 (branch, term) 文件，不能覆写旧 term 文件。
#[compio::test]
async fn old_term_file_is_not_overwritten_by_new_term_writes() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
  store.set_file_identity("main", 1);
  store.write_page(&tbl, 0, &vec![2u8; 512]).await.unwrap();

  let dir = root.join("demo.1");
  write(dir.join("data_0_main_0"), vec![1u8; 512]).unwrap();
  // 触发一次目录重扫后再写。
  store.invalidate_path_cache();
  store.write_page(&tbl, 0, &vec![3u8; 512]).await.unwrap();

  let old = read(dir.join("data_0_main_0")).unwrap();
  assert_eq!(old[0], 1, "旧 term 文件不得被新 term 写入覆写");
  let current = read(dir.join("data_0_main_1")).unwrap();
  assert_eq!(current[0], 3);
  let mut page = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page[0], 3);
}

/// io-04：当前 term 尚无该 file id 文件、目录里只有旧 term 文件时，
/// 首次写入必须新建当前 term 文件，不能把旧 term 文件当作写目标。
#[compio::test]
async fn first_write_of_new_term_does_not_reuse_old_term_file() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  // 旧 term 遗留文件：当前 identity 下还没有任何 data_0 / segment_0。
  write(dir.join("data_0_main_0"), vec![1u8; 512]).unwrap();
  write(dir.join("segment_0_main_0"), vec![2u8; 4096]).unwrap();

  store.set_file_identity("main", 1);
  store.invalidate_path_cache();
  store.write_page(&tbl, 0, &vec![3u8; 512]).await.unwrap();
  store
    .write_segment(&tbl, 0, &vec![4u8; 4096])
    .await
    .unwrap();

  assert_eq!(read(dir.join("data_0_main_0")).unwrap()[0], 1);
  assert_eq!(read(dir.join("segment_0_main_0")).unwrap()[0], 2);
  assert_eq!(read(dir.join("data_0_main_1")).unwrap()[0], 3);
  assert_eq!(read(dir.join("segment_0_main_1")).unwrap()[0], 4);
}

/// io-04：按 file id 删除必须清掉同名 id 的全部 (branch, term) 变体，
/// 否则旧 term 文件永远留在盘上无法被 GC 回收。
#[compio::test]
async fn remove_file_clears_all_identity_variants() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();
  for name in [
    "data_0_main_0",
    "data_0_main_1",
    "data_0_feature-1_2",
    "segment_0_main_0",
    "segment_0_main_1",
    "segment_0_feature-1_2",
  ] {
    let len = if name.starts_with("data_") { 512 } else { 4096 };
    write(dir.join(name), vec![7u8; len]).unwrap();
  }
  store.set_file_identity("main", 1);
  store.invalidate_path_cache();
  // GC 先 list 再 remove：list 触发扫描建立索引。
  assert_eq!(store.list_file_id_li(&tbl).await.unwrap(), vec![0]);
  assert_eq!(store.list_segment_file_id_li(&tbl).await.unwrap(), vec![0]);
  store.remove_file(&tbl, 0).await.unwrap();
  store.remove_segment_file(&tbl, 0).await.unwrap();

  let remain_li: Vec<_> = read_dir(&dir)
    .unwrap()
    .filter_map(|entry| entry.ok())
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .filter(|name| name.starts_with("data_") || name.starts_with("segment_"))
    .collect();
  assert!(remain_li.is_empty(), "残留文件: {remain_li:?}");
  assert!(store.list_file_id_li(&tbl).await.unwrap().is_empty());
  assert!(
    store
      .list_segment_file_id_li(&tbl)
      .await
      .unwrap()
      .is_empty()
  );
}

/// 新 term 的空（稀疏）文件不能遮蔽旧 term 已落盘的页：候选读到 0 字节时
/// 继续尝试更旧的 term，全部候选耗尽才报 `EndOfFile`。
#[compio::test]
async fn empty_new_term_file_does_not_mask_old_term_page() {
  let root = TempRoot::new("naming");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), 512, 2)
    .with_segment_config(4096, 2)
    .with_checksum_verify(false);
  let dir = root.join("demo.1");
  create_dir_all(&dir).unwrap();

  store.set_file_identity("b", 2);
  store.write_page(&tbl, 0, &vec![9u8; 512]).await.unwrap();

  // 新 term 只建了空文件，页仍留在旧 term 文件里。
  store.set_file_identity("b", 3);
  write(dir.join("data_0_b_3"), Vec::new()).unwrap();
  write(dir.join("data_1_b_3"), Vec::new()).unwrap();
  store.invalidate_path_cache();

  let mut page = vec![0u8; 512];
  store.read_page(&tbl, 0, &mut page).await.unwrap();
  assert_eq!(page, vec![9u8; 512]);

  // 单文件真 EOF（候选只有一个空文件）仍报 EndOfFile。
  let mut missing = vec![0u8; 512];
  assert_eq!(
    store.read_page(&tbl, 4, &mut missing).await.unwrap_err(),
    KvError::EndOfFile
  );
}
