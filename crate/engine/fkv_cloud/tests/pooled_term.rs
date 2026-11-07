//! io-15：CloudStore + Pooled 的跨 term 写读键一致性。
//!
//! 旧行为：term bump 只抬 allocator 高水位，Pooled 空闲池仍从旧 term 文件的
//! 空洞回填；新 term 的写落进旧 file id。写键按当前 identity
//! （`write_file_key` = `_main_<new>`），读键按 manifest 区间链解析的写时
//! identity（`file_key` = `_main_<old>`），本地 cache 驱逐 / 重启后新页不可读。
//! 修复后 term bump 把复用下界抬到新文件边界，新写只落新 file id，写读键一致。

use fkv_btree::BatchEntry;
use fkv_cloud::CloudStore;
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_store::{AllocMode, Store};
use fkv_types::TableIdent;

const PAGE_SIZE: usize = 512;
/// 每文件 4 页：小批量写落满 file 0，term bump 的新文件边界是 file 1。
const SHIFT: u8 = 2;
const RESTART: u16 = 16;

type Cloud = Store<CloudStore<MemStore, MemObjectStore>>;

fn cloud(remote: &MemObjectStore, term: u64, mode: AllocMode) -> Cloud {
  Store::new(
    CloudStore::new(
      MemStore::with_pages_per_file(PAGE_SIZE, SHIFT),
      remote.clone(),
      "fkv",
      SHIFT,
    ),
    PAGE_SIZE,
    RESTART,
  )
  .with_pages_per_file_shift(SHIFT)
  .with_alloc_mode(mode)
  .with_branch("main", term)
}

fn batch() -> Vec<BatchEntry> {
  (0..4u32)
    .map(|i| BatchEntry::upsert(format!("k{i}").into_bytes(), vec![i as u8 + 1; 64], 1))
    .collect()
}

/// term 3 写 + term 5 reopen + put：新页必须上传到新 file id 的 `_main_5` 对象，
/// 冷启动（空本地 cache）重开 term 5 后新旧 key 都可读。
#[compio::test]
async fn cloud_pooled_term_bump_writes_and_reads_same_key() {
  let tbl = TableIdent::new("demo", 1).unwrap();
  let remote = MemObjectStore::new();
  let entries = batch();

  let mut term3 = cloud(&remote, 3, AllocMode::Pooled);
  term3.put(&tbl, &entries).await.unwrap();
  let old_max = term3.mapper(&tbl).unwrap().max_fp_id();
  drop(term3);

  let mut term5 = cloud(&remote, 5, AllocMode::Pooled);
  term5.reopen(&tbl).await.unwrap();
  let bumped = ((old_max >> SHIFT) + 1) << SHIFT;
  // 历史 term 的 `manifest_main_<term>` 是活跃 manifest 历史，不是子分支预留块：
  // 不得把复用下界抬到 1024 文件块的末尾。
  assert_eq!(
    term5.mapper(&tbl).unwrap().max_fp_id(),
    bumped,
    "reopen 下界必须停在新 term 的文件边界"
  );
  term5
    .put(
      &tbl,
      &[BatchEntry::upsert(b"new".to_vec(), b"N".to_vec(), 2)],
    )
    .await
    .unwrap();
  let new_max = term5.mapper(&tbl).unwrap().max_fp_id();
  assert!(
    new_max > bumped,
    "term bump 后不得复用旧 term 文件的空洞: max={new_max} bumped={bumped}"
  );

  // 写键必须是新 file id 的当前 identity 对象；旧 file id 下不得出现 `_main_5`。
  let dir = format!("fkv/{tbl}");
  let new_file_id = bumped >> SHIFT;
  let new_key = format!("{dir}/data_{new_file_id}_main_5");
  assert!(
    remote.get(&new_key).await.unwrap().is_some(),
    "新 term 写入必须上传到当前 identity 的新 file id: {new_key}"
  );
  assert!(
    remote
      .get(&format!("{dir}/data_0_main_5"))
      .await
      .unwrap()
      .is_none(),
    "旧 file id 不得出现新 term 文件"
  );
  assert!(
    remote
      .get(&format!("{dir}/data_0_main_3"))
      .await
      .unwrap()
      .is_some(),
    "旧 term 对象必须保留"
  );

  // 冷启动（空本地 cache）以 term 5 重开：新区间归属链让新旧 key 都按正确对象读。
  let mut cold = cloud(&remote, 5, AllocMode::Pooled);
  cold.reopen(&tbl).await.unwrap();
  assert_eq!(cold.get(&tbl, b"new").await.unwrap(), Some(b"N".to_vec()));
  for entry in &entries {
    assert_eq!(
      cold.get(&tbl, &entry.key).await.unwrap(),
      Some(entry.value.clone()),
      "旧 term key 在新 term 冷启动后必须可读"
    );
  }
}
