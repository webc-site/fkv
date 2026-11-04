//! 云缓存预热：把某个 table 的 manifest / data / segment 文件拉到本地 cache。

use fkv_cloud::CloudStore;
use fkv_error::Result;
use fkv_io::{FileIo, ManifestIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::TableIdent;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrewarmStats {
  pub manifest_present: bool,
  pub data_file_num: usize,
  pub segment_file_num: usize,
  pub missing_num: usize,
}

impl PrewarmStats {
  pub fn file_num(&self) -> usize {
    self.data_file_num + self.segment_file_num + usize::from(self.manifest_present)
  }
}

/// 预热单个 table。先 manifest，再 data，最后 segment。
pub async fn prewarm_table<I, O>(cloud: &CloudStore<I, O>, tbl: &TableIdent) -> Result<PrewarmStats>
where
  I: FileIo + ManifestIo + SegmentIo,
  O: ObjectStore,
{
  let mut stats = PrewarmStats {
    manifest_present: cloud.prewarm_manifest(tbl).await?,
    ..PrewarmStats::default()
  };
  if !stats.manifest_present {
    stats.missing_num += 1;
  }
  for file_id in cloud.remote_data_file_id_li(tbl).await? {
    if cloud.prewarm_data_file(tbl, file_id).await? {
      stats.data_file_num += 1;
    } else {
      stats.missing_num += 1;
    }
  }
  for file_id in cloud.remote_segment_file_id_li(tbl).await? {
    if cloud.prewarm_segment_file(tbl, file_id).await? {
      stats.segment_file_num += 1;
    } else {
      stats.missing_num += 1;
    }
  }
  Ok(stats)
}

pub async fn prewarm_tables<I, O>(
  cloud: &CloudStore<I, O>,
  table_li: &[TableIdent],
) -> Result<Vec<(TableIdent, PrewarmStats)>>
where
  I: FileIo + ManifestIo + SegmentIo,
  O: ObjectStore,
{
  let mut result_li = Vec::with_capacity(table_li.len());
  for tbl in table_li {
    result_li.push((tbl.clone(), prewarm_table(cloud, tbl).await?));
  }
  Ok(result_li)
}
