//! 包装层（`MeteredIo`）必须逐方法下探内层。
//!
//! 漏转发时 trait 默认实现会静默顶替后端能力：`set_file_range_li` 变 no-op
//! 会让 Cloud 的 per-file term 范围丢失，跨 term 读按当前 identity 选错对象
//! key；`list_file_variants` 回退后 identity 全是 `None`；`write_segment_len`
//! 会把定长段落盘字节数记少。本测试用记录型 inner 断言这三条都透传。

use std::sync::{Arc, Mutex};

use fkv_error::Result;
use fkv_io::{FileIo, FileVariant, ManifestIo, PageIo, SegmentIo};
use fkv_metrics::{MeteredIo, ShardMetric};
use fkv_types::{BranchFileRange, FileId, FilePageId, TableIdent};

/// 定长段后端：任意写入都补零到 4096。
const SEGMENT_SIZE: u64 = 4096;

#[derive(Debug, Default)]
struct Recorder {
  /// 最近一次 `set_file_range_li` 的 `(tbl, range_li)`。
  range: Mutex<Option<(TableIdent, Vec<BranchFileRange>)>>,
  /// `list_sidecar_file_name_li` 的返回集。
  sidecar_li: Mutex<Vec<String>>,
  /// 记录收到的 `remove_sidecar_file_name` 调用。
  removed_sidecar: Mutex<Vec<(TableIdent, String)>>,
}

impl PageIo for Recorder {
  fn set_file_range_li(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    *self.range.lock().unwrap() = Some((tbl.clone(), range_li.to_vec()));
  }

  async fn read_page(&self, _tbl: &TableIdent, _id: FilePageId, _dst: &mut [u8]) -> Result<()> {
    Ok(())
  }

  async fn write_page(&self, _tbl: &TableIdent, _id: FilePageId, _src: &[u8]) -> Result<()> {
    Ok(())
  }

  async fn sync_data(&self, _tbl: &TableIdent) -> Result<()> {
    Ok(())
  }
}

impl SegmentIo for Recorder {
  fn write_segment_len(&self, _src_len: usize) -> u64 {
    SEGMENT_SIZE
  }

  async fn read_segment(&self, _tbl: &TableIdent, _id: FilePageId, _dst: &mut [u8]) -> Result<()> {
    Ok(())
  }

  async fn write_segment(&self, _tbl: &TableIdent, _id: FilePageId, _src: &[u8]) -> Result<()> {
    Ok(())
  }
}

impl FileIo for Recorder {
  async fn list_file_id_li(&self, _tbl: &TableIdent) -> Result<Vec<FileId>> {
    Ok(Vec::new())
  }

  async fn list_file_variants(&self, _tbl: &TableIdent) -> Result<Vec<FileVariant>> {
    Ok(vec![FileVariant {
      file_id: 7,
      identity: Some(("feature".into(), 3)),
    }])
  }

  async fn read_file(&self, _tbl: &TableIdent, _id: FileId) -> Result<Option<Vec<u8>>> {
    Ok(None)
  }

  async fn write_file(&self, _tbl: &TableIdent, _id: FileId, _bytes: &[u8]) -> Result<()> {
    Ok(())
  }

  async fn file_size(&self, _tbl: &TableIdent, _id: FileId) -> Result<Option<u64>> {
    Ok(None)
  }

  async fn remove_file(&self, _tbl: &TableIdent, _id: FileId) -> Result<()> {
    Ok(())
  }

  async fn list_segment_file_id_li(&self, _tbl: &TableIdent) -> Result<Vec<FileId>> {
    Ok(Vec::new())
  }

  async fn remove_segment_file(&self, _tbl: &TableIdent, _id: FileId) -> Result<()> {
    Ok(())
  }
}

impl ManifestIo for Recorder {
  async fn append_manifest(&self, _tbl: &TableIdent, _record: &[u8]) -> Result<()> {
    Ok(())
  }

  async fn switch_manifest(&self, _tbl: &TableIdent, _record: &[u8]) -> Result<()> {
    Ok(())
  }

  async fn read_manifest(&self, _tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    Ok(None)
  }

  async fn drop_manifest(&self, _tbl: &TableIdent) -> Result<()> {
    Ok(())
  }

  async fn write_manifest_file(
    &self,
    _tbl: &TableIdent,
    _name: &str,
    _record: &[u8],
  ) -> Result<()> {
    Ok(())
  }

  async fn append_manifest_file(
    &self,
    _tbl: &TableIdent,
    _name: &str,
    _record: &[u8],
  ) -> Result<()> {
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, _tbl: &TableIdent) -> Result<Vec<String>> {
    Ok(Vec::new())
  }

  async fn list_sidecar_file_name_li(&self, _tbl: &TableIdent) -> Result<Vec<String>> {
    Ok(self.sidecar_li.lock().unwrap().clone())
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self
      .removed_sidecar
      .lock()
      .unwrap()
      .push((tbl.clone(), name.to_owned()));
    Ok(())
  }

  async fn read_manifest_file(&self, _tbl: &TableIdent, _name: &str) -> Result<Option<Vec<u8>>> {
    Ok(None)
  }

  async fn remove_manifest_file(&self, _tbl: &TableIdent, _name: &str) -> Result<()> {
    Ok(())
  }
}

fn metered() -> MeteredIo<Recorder> {
  MeteredIo::new(Recorder::default(), Arc::new(ShardMetric::new(0)))
}

#[test]
fn set_file_range_li_is_forwarded() {
  let io = metered();
  let tbl = TableIdent::new("demo", 2).unwrap();
  let range_li = [
    BranchFileRange {
      branch_name: "feature".into(),
      term: 3,
      max_file_id: 9,
      max_segment_file_id: 4,
    },
    BranchFileRange {
      branch_name: "main".into(),
      term: 1,
      max_file_id: 5,
      max_segment_file_id: 2,
    },
  ];

  io.set_file_range_li(&tbl, &range_li);

  let recorded = io.inner().range.lock().unwrap().clone();
  assert_eq!(recorded, Some((tbl, range_li.to_vec())));
}

#[compio::test]
async fn list_file_variants_keeps_identity() {
  let io = metered();
  let tbl = TableIdent::new("demo", 2).unwrap();

  let variant_li = io.list_file_variants(&tbl).await.unwrap();

  assert_eq!(
    variant_li,
    vec![FileVariant {
      file_id: 7,
      identity: Some(("feature".into(), 3)),
    }]
  );
}

#[test]
fn write_segment_len_is_forwarded() {
  let io = metered();
  assert_eq!(io.write_segment_len(10), SEGMENT_SIZE);
}

/// 旁路文件枚举 / 删除必须下探内层：漏转发会让 segment 完成标记永久残留。
#[compio::test]
async fn sidecar_file_calls_are_forwarded() {
  let io = metered();
  let tbl = TableIdent::new("demo", 2).unwrap();
  *io.inner().sidecar_li.lock().unwrap() = vec!["segcomplete_7".to_owned()];

  assert_eq!(
    io.list_sidecar_file_name_li(&tbl).await.unwrap(),
    vec!["segcomplete_7".to_owned()]
  );
  io.remove_sidecar_file_name(&tbl, "segcomplete_7")
    .await
    .unwrap();
  assert_eq!(
    io.inner().removed_sidecar.lock().unwrap().clone(),
    vec![(tbl, "segcomplete_7".to_owned())]
  );
}

#[compio::test]
async fn page_metrics_still_counted_after_forwarding() {
  let metrics = Arc::new(ShardMetric::new(0));
  let io = MeteredIo::new(Recorder::default(), metrics.clone());
  let tbl = TableIdent::new("demo", 2).unwrap();
  let mut buf_li = vec![vec![0u8; 8], vec![0u8; 8]];

  io.read_page_li(&tbl, &[1, 2], &mut buf_li).await.unwrap();

  let snapshot = metrics.snapshot();
  // 批量读按一次 op 计、字节按全部页累计（与 write_page_li 同口径）。
  assert_eq!(snapshot.page_reads, 1);
  assert_eq!(snapshot.bytes_read, 16);
}
