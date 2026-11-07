use fkv_error::KvError;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_task_reopen::{reopen, split_manifest, split_manifest_with_len};
use fkv_types::{TableIdent, encode_file_page_id};

#[test]
fn reopen_snapshot_and_log() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let branch = BranchManifestMetadata::new("main", 0);
    let mut builder = ManifestBuilder::new();
    let snapshot = builder
      .snapshot(
        SnapshotArg {
          root_id: 1,
          ttl_root: 2,
          max_fp_id: 3,
          dict_bytes: b"",
          branch_metadata: &branch,
          max_segment_fp_id: 0,
        },
        &[encode_file_page_id(5)],
        None,
      )
      .unwrap();
    assert_eq!(split_manifest(&snapshot).unwrap().len(), 1);

    let mut builder = ManifestBuilder::new();
    builder.update_mapping(1, encode_file_page_id(9));
    builder.append_branch_metadata(&branch).unwrap();
    builder.append_segment_mapping(&0u32.to_le_bytes());
    let log = builder.finalize(3, 4).unwrap();

    store.append_manifest(&tbl, &snapshot).await.unwrap();
    store.append_manifest(&tbl, &log).await.unwrap();
    let state = reopen(&store, &tbl).await.unwrap();
    assert_eq!(state.root_id, 3);
    assert_eq!(state.ttl_root_id, 4);
    assert_eq!(state.mapping_tbl.get(0).unwrap(), encode_file_page_id(5));
    assert_eq!(state.mapping_tbl.get(1).unwrap(), encode_file_page_id(9));
  });
}

fn make_snapshot() -> Vec<u8> {
  let branch = BranchManifestMetadata::new("main", 0);
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 3,
        dict_bytes: b"",
        branch_metadata: &branch,
        max_segment_fp_id: 0,
      },
      &[encode_file_page_id(5)],
      None,
    )
    .unwrap()
}

fn make_log(page_id: u32, fp: u64) -> Vec<u8> {
  let branch = BranchManifestMetadata::new("main", 0);
  let mut builder = ManifestBuilder::new();
  builder.update_mapping(page_id, encode_file_page_id(fp));
  builder.append_branch_metadata(&branch).unwrap();
  builder.append_segment_mapping(&0u32.to_le_bytes());
  builder.finalize(3, 4).unwrap()
}

/// fmt-08：torn padding / torn body 丢弃最后一条 record，之前记录保留。
#[test]
fn split_manifest_torn_tail_discards_last_record() {
  let snapshot = make_snapshot();
  let log = make_log(1, 9);
  let mut bytes = snapshot.clone();
  bytes.extend_from_slice(&log);
  assert_eq!(split_manifest(&bytes).unwrap().len(), 2);

  // 去掉最后一个 padding 字节 → body 完好、padding 缺失。
  let torn_padding = &bytes[..bytes.len() - 1];
  assert_eq!(split_manifest(torn_padding).unwrap().len(), 1);
  // sem-14 有意差异：Rust 的追加基准停在最后一条完整 record 的对齐末尾。
  // C++ `Replayer::ParseNextRecord` 则把 `file_size_` 推进到被丢弃 record 的
  // 对齐末尾（= `bytes.len()`，由 `eloqstore/tests/manifest.cpp` 的
  // `SECTION("truncated trailing record")` 钉住），下一次 append 在 EOF 后留
  // 零洞，再 reopen 会 replay 这条未提交 record。Rust 截断到这里后 append
  // 覆盖撕裂 record，恢复幂等。
  let (record_li, torn_valid_len) = split_manifest_with_len(torn_padding).unwrap();
  assert_eq!(record_li.len(), 1);
  assert_eq!(torn_valid_len, snapshot.len());
  assert!(torn_valid_len < torn_padding.len());

  // 去掉部分 body → torn body。
  let torn_body = &bytes[..bytes.len() - log.len() / 2];
  assert_eq!(split_manifest(torn_body).unwrap().len(), 1);
}

/// fmt-08：损坏中间 record 后又出现完好 record → Corrupted；损坏尾部被忽略。
#[test]
fn split_manifest_corruption_between_records() {
  let snapshot = make_snapshot();
  let log1 = make_log(1, 9);
  let log2 = make_log(2, 10);

  // 尾部（log2）损坏：忽略坏 record，保留 snapshot + log1。
  let mut bytes = snapshot.clone();
  bytes.extend_from_slice(&log1);
  bytes.extend_from_slice(&log2);
  let corrupt_at = snapshot.len() + log1.len() + 10;
  bytes[corrupt_at] ^= 0xff;
  assert_eq!(split_manifest(&bytes).unwrap().len(), 2);

  // 中间（log1）损坏且后面还有完好 log2 → Corrupted。
  let mut bytes = snapshot.clone();
  bytes.extend_from_slice(&log1);
  bytes.extend_from_slice(&log2);
  let corrupt_at = snapshot.len() + 10;
  bytes[corrupt_at] ^= 0xff;
  assert_eq!(split_manifest(&bytes).unwrap_err(), KvError::Corrupted);

  // 首个 record 损坏 → Corrupted。
  let mut bytes = snapshot.clone();
  bytes[10] ^= 0xff;
  assert_eq!(split_manifest(&bytes).unwrap_err(), KvError::Corrupted);
}

/// core-24：存在但无完整 record 的 manifest 视为损坏，不能降级为空表。
#[test]
fn short_manifest_is_corrupted() {
  assert_eq!(split_manifest(&[0u8; 3]).unwrap_err(), KvError::Corrupted);
  assert_eq!(split_manifest(&[0u8; 7]).unwrap_err(), KvError::Corrupted);
}

/// core-20：尾随撕裂字节不计入有效长度，后续 append 应从这里继续。
#[test]
fn valid_len_excludes_torn_tail() {
  let snapshot = make_snapshot();
  let mut bytes = snapshot.clone();
  bytes.extend_from_slice(&[0u8; 100]);
  let (record_li, valid_len) = split_manifest_with_len(&bytes).unwrap();
  assert_eq!(record_li.len(), 1);
  assert_eq!(valid_len, snapshot.len());
  assert!(valid_len < bytes.len());
}

/// 构造 `max_fp_id < max(mapping) + 1` 的 snapshot：水位 3，映射引用物理页 5 / 100。
fn snapshot_with_low_watermark() -> Vec<u8> {
  let branch = BranchManifestMetadata::new("main", 0);
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 3,
        dict_bytes: b"",
        branch_metadata: &branch,
        max_segment_fp_id: 0,
      },
      &[encode_file_page_id(5), encode_file_page_id(100)],
      None,
    )
    .unwrap()
}

/// l2-05(b)：snapshot 自报水位低于映射最大页时，replay 必须按映射抬升。
///
/// C++ `Replayer::DeserializeSnapshot` 只取首 varint
/// （`eloqstore/src/replayer.cpp:151-152`），**不**按 snapshot 的映射抬升；它依赖
/// 写侧不变量「snapshot 的 `max_fp_id` = 同一 mapper 分配器高水位」：
/// `FilePageAllocator::Allocate()` 恒 `max_fp_id_++`
/// （`src/storage/page_mapper.cpp:654-658`），快照写出该值
/// （`src/tasks/write_task.cpp:757-758`、`:797-808`，`src/tasks/background_write.cpp:558`），
/// `AppendAllocator::UpdateStat` 只把水位向上 snap（`:667-678`）；非 append 分支
/// 还断言该不变量（`src/replayer.cpp:388`），默认构建（无 `-DNDEBUG`）下违约即 abort。
///
/// fkv 侧同一次扫描抬升是**单侧加固**：manifest 水位低于被引用页时，不抬升会让
/// 分配器从低水位起分配——`AppendAllocator(max_fp_id_)` 与 `PooledFilePages`
/// 的下一次 `Allocate()` 就是水位本身——从而交出仍被引用的页 id，两个逻辑页
/// 指向同一物理页。登记见 `COMPAT_BASELINE`「reopen 时的 `max_fp_id` 抬升」条，
/// 本用例锁定该行为，防止后续「对齐 C++」把它删掉。
#[test]
fn snapshot_watermark_is_raised_to_cover_mapping_max() {
  let state = fkv_task_reopen::replay_manifest_bytes(&snapshot_with_low_watermark()).unwrap();
  assert_eq!(
    state.max_fp_id, 101,
    "水位必须抬到映射最大物理页 + 1（5 / 100 中的 100 + 1）"
  );
}

/// l2-05(b) 的对照面：写侧不变量成立时（水位 >= 映射最大页 + 1），抬升是恒等变换，
/// 两侧 reopen 得到同一水位。差分对拍以该形状的 manifest 为准。
#[test]
fn well_formed_snapshot_watermark_is_unchanged() {
  let branch = BranchManifestMetadata::new("main", 0);
  let snapshot = ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id: 1,
        ttl_root: 2,
        max_fp_id: 101,
        dict_bytes: b"",
        branch_metadata: &branch,
        max_segment_fp_id: 0,
      },
      &[encode_file_page_id(5), encode_file_page_id(100)],
      None,
    )
    .unwrap();
  let state = fkv_task_reopen::replay_manifest_bytes(&snapshot).unwrap();
  assert_eq!(state.max_fp_id, 101);
}
