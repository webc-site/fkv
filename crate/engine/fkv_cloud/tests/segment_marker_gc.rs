//! segment 旁路完成标记的生命周期（第 2 项）：
//!
//! - `drop_manifest` 必须清掉表级 scheme 与单文件标记，否则空分区目录里的
//!   `segcomplete*` 会永远挡住目录回收；
//! - LocalGC 删 segment 文件必须连带删标记，否则 `file_id` 复用后 stale 标记
//!   会把新的半装文件判成完整，读零 / flush 把零上传成云端权威。

use std::path::Path;

use bytes::Bytes;
use fkv_cloud::CloudStore;
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_io_compio::CompioStore;
use fkv_object::{MemObjectStore, ObjectStore};
use fkv_testkit::TempRoot;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

const PAGE_SIZE: usize = 512;
const SEGMENT_SIZE: usize = 4096;
const SEGMENT_SHIFT: u8 = 1;
/// `SEGMENT_SHIFT = 1` → 每个 segment 文件 2 槽，名义 8 KiB。
const SEGMENT_FILE_BYTES: usize = SEGMENT_SIZE * 2;
const SEGMENT_KEY: &str = "fkv/demo.1/segment_0_main_0";

fn cloud(root: &Path, remote: &MemObjectStore) -> CloudStore<CompioStore, MemObjectStore> {
  CloudStore::new(
    CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT),
    remote.clone(),
    "fkv",
    DEFAULT_PAGES_PER_FILE_SHIFT,
  )
  .with_segment_config(SEGMENT_SIZE, SEGMENT_SHIFT)
}

/// 播种远端完整 segment 并装进本地 cache：会建立 scheme + 单文件标记。
async fn install_segment(
  cloud: &CloudStore<CompioStore, MemObjectStore>,
  remote: &MemObjectStore,
  tbl: &TableIdent,
) {
  remote
    .put(
      SEGMENT_KEY,
      Bytes::from(vec![7u8; SEGMENT_FILE_BYTES]),
      None,
      false,
    )
    .await
    .unwrap();
  assert!(
    remote.get(SEGMENT_KEY).await.unwrap().is_some(),
    "远端播种必须可见"
  );
  assert!(cloud.prewarm_segment_file(tbl, 0).await.unwrap());
}

#[compio::test]
async fn drop_clears_markers_and_partition_dir_is_reclaimed() {
  let temp = TempRoot::new("cloud_marker_drop");
  let root = temp.path();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let remote = MemObjectStore::new();
  let cloud = cloud(root, &remote);
  install_segment(&cloud, &remote, &tbl).await;

  let probe = CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT);
  let name_li = probe.list_sidecar_file_name_li(&tbl).await.unwrap();
  assert!(
    name_li.iter().any(|name| name == "segcomplete"),
    "{name_li:?}"
  );
  assert!(
    name_li.iter().any(|name| name == "segcomplete_0"),
    "{name_li:?}"
  );
  // 有标记时分区目录非空，不能回收。
  assert!(!cloud.try_remove_empty_partition_dir(&tbl).await.unwrap());

  ManifestIo::drop_manifest(&cloud, &tbl).await.unwrap();
  assert!(
    !probe
      .list_sidecar_file_name_li(&tbl)
      .await
      .unwrap()
      .iter()
      .any(|name| name.starts_with("segcomplete")),
    "drop 之后不得残留 scheme / 单文件标记"
  );

  // LocalGc 只删本地 cache 文件；标记已由 drop 清掉。
  FileIo::remove_local_segment_file(&cloud, &tbl, 0)
    .await
    .unwrap();
  assert!(
    cloud.try_remove_empty_partition_dir(&tbl).await.unwrap(),
    "drop + LocalGc 后空分区目录必须可回收"
  );
  assert!(!root.join("demo.1").exists());
}

#[compio::test]
async fn reused_file_id_is_not_fooled_by_stale_marker() {
  let temp = TempRoot::new("cloud_marker_reuse");
  let root = temp.path();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let remote = MemObjectStore::new();
  let cloud = cloud(root, &remote);
  install_segment(&cloud, &remote, &tbl).await;

  // LocalGc 删掉 segment 文件：标记必须一起删，否则会被复用的 file_id 命中。
  FileIo::remove_local_segment_file(&cloud, &tbl, 0)
    .await
    .unwrap();

  // 同 file_id 重新落一个「尾槽可读但只有零」的半装文件（file_id 复用现场）。
  let probe = CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT);
  probe
    .write_segment(&tbl, 0, &[9u8; SEGMENT_SIZE])
    .await
    .unwrap();
  probe
    .write_segment(&tbl, 1, &[0u8; SEGMENT_SIZE])
    .await
    .unwrap();

  assert!(
    !cloud
      .adopt_local_file(&tbl, 0, true, SEGMENT_FILE_BYTES as u64)
      .await
      .unwrap(),
    "GC 删除后 stale 标记不得把复用的半装文件判完整"
  );
}

/// 第 1 项：flush 上传成功后必须给本地新建的段文件写完成标记。
///
/// 回退即失败：不写标记时 `segment_state` 判半装，淘汰 / 重启后整文件重下。
#[compio::test]
async fn flush_writes_segment_marker() {
  let temp = TempRoot::new("cloud_marker_flush");
  let root = temp.path();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let remote = MemObjectStore::new();
  let cloud = cloud(root, &remote);
  // 本实例新写段文件，远端没有对象：install 回填不会覆盖它。
  SegmentIo::write_segment(&cloud, &tbl, 0, &[5u8; SEGMENT_SIZE])
    .await
    .unwrap();
  PageIo::sync_data(&cloud, &tbl).await.unwrap();
  assert!(
    remote.get(SEGMENT_KEY).await.unwrap().is_some(),
    "flush 必须上传段文件"
  );

  let probe = CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT);
  let name_li = probe.list_sidecar_file_name_li(&tbl).await.unwrap();
  assert!(
    name_li.iter().any(|name| name == "segcomplete_0"),
    "flush 成功后必须写完成标记: {name_li:?}"
  );
}

/// 第 5 项：淘汰删除段文件必须连带删完成标记，否则按淘汰量线性泄漏小文件。
///
/// 回退即失败：只删段文件时标记残留，断言看到 `segcomplete_0`。
#[compio::test]
async fn drain_evictions_removes_segment_marker() {
  let temp = TempRoot::new("cloud_marker_evict");
  let root = temp.path();
  let tbl = TableIdent::new("demo", 1).unwrap();
  let remote = MemObjectStore::new();
  // 预算 1 字节：装完立即进待删队列并触发 drain。
  let cloud = cloud(root, &remote).with_cache_budget(1);
  install_segment(&cloud, &remote, &tbl).await;
  cloud.drain_evictions().await.unwrap();

  let probe = CompioStore::new(root.to_path_buf(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT);
  let name_li = probe.list_sidecar_file_name_li(&tbl).await.unwrap();
  assert!(
    !name_li.iter().any(|name| name == "segcomplete_0"),
    "淘汰删段必须连带删完成标记: {name_li:?}"
  );
  // 表级 scheme 标记仍由 drop / invalidate 负责，本轮不动。
  assert!(
    name_li.iter().any(|name| name == "segcomplete"),
    "表级 scheme 标记不应在单文件淘汰中被删: {name_li:?}"
  );
}
