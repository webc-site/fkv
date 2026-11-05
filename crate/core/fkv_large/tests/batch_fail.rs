//! 批量段写中途失败：本 value 分配的逻辑段必须全部释放。
use fkv_error::KvError;
use fkv_failpoint::set_spec;
use fkv_io_compio::CompioStore;
use fkv_large::{SegmentLayout, read_value, write_value};
use fkv_mapping::{AllocMode, PageMapper};
use fkv_testkit::TempRoot;
use fkv_types::{TableIdent, is_file_page_id};

#[compio::test]
async fn batch_write_failure_frees_allocated_segments() {
  let root = TempRoot::new("large_batch_fail");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let layout = SegmentLayout::new(4096, 2).unwrap();
  let store = CompioStore::new(
    root.path().to_path_buf(),
    512,
    fkv_types::DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(4096, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let value: Vec<u8> = (0..4096 * 17 + 9)
    .map(|index| (index % 251) as u8)
    .collect();

  // 18 段跨 3 批，第 3 次段写失败：整条 value 的段全部回收。
  set_spec("WriteSegments@3");
  assert_eq!(
    write_value(&store, &tbl, &mut mapper, layout, &value, b"")
      .await
      .unwrap_err(),
    KvError::IoFail
  );
  set_spec("");
  // 已分配的 18 个物理段全部释放，没有残留活跃 `FilePageId`。
  let live = mapper
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .count();
  assert_eq!(live, 0);

  let written = write_value(&store, &tbl, &mut mapper, layout, &value, b"")
    .await
    .unwrap();
  assert_eq!(written.segment_id_li.len(), 18);
  let read = read_value(&store, &tbl, &mapper, layout, &written.content)
    .await
    .unwrap();
  assert_eq!(read.value, value);
}
