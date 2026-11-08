//! l2-10：`Replayer::GetMapper` 分配器重建的 1:1 锁定。
//!
//! C++ `Replayer::GetMapper`（`eloqstore/src/replayer.cpp:305-430`）是 reopen /
//! install 时的唯一分配器重建入口：
//!
//! - append：`min_file_id` 取最小活跃页所在文件；`empty_file_cnt` 统计
//!   `[min_file_id, max_file_id]` 内没有活跃页的文件数；空映射时
//!   `min_file_id = max_fp_id >> shift`、`empty_file_cnt = 0`
//!   （`AppendAllocator(opts, min, max, empty)`，`src/replayer.cpp:364-391`）；
//! - pooled：把 `[0, max_fp_id)` 内不在 using 集合的页全部回填
//!   `PooledFilePages`（`Allocate()` 从 `free_ids` 尾部弹出 ⇒ 从最高空闲页降序），
//!   映射表带 `file_ranges` 时只回填当前 branch 名下的文件区间
//!   （`FileIdInBranch`，`src/replayer.cpp:393-427`）；
//! - 两种模式都先把非 `FilePageId` 槽位串成逻辑页空闲链（`FreePage`）。
//!
//! Rust 的等价逻辑在 `Store::install_replay_state`（`manifest.rs`）+
//! `PageMapper::update_stat_by_scan` + `rebuild_pooled_free_file_page_li`。本文件用
//! [`ManifestBuilder::snapshot`] 构造与 C++ `ManifestBuilder::Snapshot` 同布局的
//! manifest（格式由 `fixtures/cpp/manifest/*` 逐字节钉住），锁定接管后的重建结果
//! 与续写行为，防止后人「对齐 C++」时删掉 term bump / 分支过滤 / 统计回写。
//!
//! 已登记的**有意差异**不在本文件覆盖范围（见 `COMPAT_BASELINE`）：跨 term 的
//! `reuse_floor`（io-15）、Local 采纳宿主 term（l2-05a）、按映射抬升 `max_fp_id`
//! （l2-05b）、孤儿归档块保护（sem-28）。本文件只锁「同 identity（branch + term
//! 相同）、无孤儿归档」这一可对拍形状。

use std::collections::BTreeSet;

use fkv_btree::BatchEntry;
use fkv_io::ManifestIo;
use fkv_io_mem::MemStore;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArg};
use fkv_store::{AllocMode, Store};
use fkv_types::{
  BranchFileRange, INVALID_VALUE, NULL_PAGE, PageId, TableIdent, decode_id, encode_file_page_id,
  encode_page_id, is_file_page_id, manifest_file_name,
};

const PAGE_SIZE: usize = 512;
const RESTART: u16 = 16;
/// 4 页 / file：小 id 即可覆盖多个文件区间与整文件空洞。
const SHIFT: u8 = 2;
/// 4 个文件（file 0..=3）的页空间，对齐 C++ 夹具的 `max_fp_id` 量纲。
const MAX_FP: u64 = 16;

fn mem() -> MemStore {
  MemStore::with_pages_per_file(PAGE_SIZE, SHIFT)
}

fn store(io: MemStore, mode: AllocMode) -> Store<MemStore> {
  Store::new(io, PAGE_SIZE, RESTART)
    .with_pages_per_file_shift(SHIFT)
    .with_alloc_mode(mode)
}

fn record(
  max_fp_id: u64,
  mapping_value_li: &[u64],
  metadata: &BranchManifestMetadata,
  root_id: PageId,
) -> Vec<u8> {
  ManifestBuilder::new()
    .snapshot(
      SnapshotArg {
        root_id,
        ttl_root: NULL_PAGE,
        max_fp_id,
        dict_bytes: &[],
        branch_metadata: metadata,
        max_segment_fp_id: 0,
      },
      mapping_value_li,
      None,
    )
    .unwrap()
}

fn entry(key: &str, value: &[u8], ts: u64) -> BatchEntry {
  BatchEntry::upsert(key.as_bytes().to_vec(), value.to_vec(), ts)
}

fn live_pages(store: &Store<MemStore>, tbl: &TableIdent) -> BTreeSet<u64> {
  store
    .mapper(tbl)
    .expect("mapper")
    .tbl()
    .iter_value_li()
    .filter(|value| is_file_page_id(*value))
    .map(decode_id)
    .collect()
}

/// C++ `GetMapper` 的 append 分支：`min_file_id` / `empty_file_cnt` 由映射反推，
/// 空映射从尾文件起算；续写从水位本身开始（`Allocate()` 恒 `max_fp_id_++`）。
#[test]
fn append_reopen_rebuilds_min_file_and_hole_stat() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l210_append", 1).unwrap();
    let io = mem();
    // 4 个逻辑页：活跃页 5（file 1）与 12（file 3），file 2 整文件为空。
    // 这正是 C++ `AppendAllocator(min_file_id=1, max_fp_id=16, empty_cnt=1)` 的输入。
    io.switch_manifest(
      &tbl,
      &record(
        MAX_FP,
        &[
          INVALID_VALUE,
          encode_file_page_id(5),
          INVALID_VALUE,
          encode_file_page_id(12),
        ],
        &BranchManifestMetadata::new("main", 0),
        NULL_PAGE,
      ),
    )
    .await
    .unwrap();

    let mut store = store(io, AllocMode::Append);
    store.prepare_attach("main", 0).unwrap();
    store.reopen(&tbl).await.unwrap();

    {
      let mapper = store.mapper(&tbl).unwrap();
      assert_eq!(
        mapper.max_fp_id(),
        MAX_FP,
        "同 term、写侧水位不变量成立 ⇒ 不抬升"
      );
      assert_eq!(mapper.min_file_id(), 1, "min_file_id = 最小活跃页 >> shift");
      // C++ `(16 - 1 个文件的起点 4) - 1 个空文件 × 4` = 8。
      assert_eq!(mapper.space_size(SHIFT), 8, "空文件必须从 space_size 扣除");
    }

    // 分配探针：append 的下一笔就是水位本身，旧页永不复用。
    let mut probe = store.mapper(&tbl).unwrap().clone();
    let page_id = probe.allocate_page();
    assert_eq!(probe.file_page_id(page_id).unwrap(), MAX_FP);
    assert_eq!(probe.max_fp_id(), MAX_FP + 1);
  });
}

/// sem-42：旧 fkv snapshot 的空闲链尾是 `encode_page_id(NULL_PAGE)`（C++ 为
/// `InvalidValue`）。reopen 重建必须归一为 C++ 编码，且分配顺序不变——不得把
/// `7` 经 `decode_id` 误解成逻辑页 0。
#[test]
fn legacy_free_chain_tail_reopens_with_cpp_sentinel() {
  pollster::block_on(async {
    let tbl = TableIdent::new("sem42_legacy_tail", 1).unwrap();
    let io = mem();
    io.switch_manifest(
      &tbl,
      &record(
        MAX_FP,
        &[
          encode_page_id(NULL_PAGE),
          encode_page_id(0),
          encode_file_page_id(2),
          INVALID_VALUE,
        ],
        &BranchManifestMetadata::new("main", 0),
        NULL_PAGE,
      ),
    )
    .await
    .unwrap();

    let mut store = store(io, AllocMode::Append);
    store.prepare_attach("main", 0).unwrap();
    store.reopen(&tbl).await.unwrap();
    let mapper = store.mapper(&tbl).unwrap();
    // 旧尾哨兵归一为 `InvalidValue`，链节保持 `PageId` 编码。
    assert_eq!(
      mapper.tbl().get(0).unwrap(),
      INVALID_VALUE,
      "旧链尾必须归一"
    );
    assert_eq!(mapper.tbl().get(1).unwrap(), encode_page_id(0));
    assert_eq!(mapper.free_page_cnt(), 3);

    // 空闲链 3 → 1 → 0（升序重建，头为最大空闲页）；之后才 append。
    let mut probe = mapper.clone();
    assert_eq!(probe.allocate_page(), 3);
    assert_eq!(probe.allocate_page(), 1);
    assert_eq!(probe.allocate_page(), 0);
    assert_eq!(probe.allocate_page(), 4);
  });
}

/// C++ 空映射 reopen：`min_file_id = max_fp_id >> shift`、`empty_file_cnt = 0`，
/// `space_size` 只剩尾文件残量（对齐时 0）。
#[test]
fn append_reopen_empty_mapping_starts_from_tail_file() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l210_append_empty", 1).unwrap();
    let io = mem();
    io.switch_manifest(
      &tbl,
      &record(
        MAX_FP,
        &[INVALID_VALUE, INVALID_VALUE],
        &BranchManifestMetadata::new("main", 0),
        NULL_PAGE,
      ),
    )
    .await
    .unwrap();

    let mut store = store(io, AllocMode::Append);
    store.prepare_attach("main", 0).unwrap();
    store.reopen(&tbl).await.unwrap();
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.max_fp_id(), MAX_FP);
    assert_eq!(mapper.min_file_id(), MAX_FP >> SHIFT);
    assert_eq!(mapper.space_size(SHIFT), 0);
  });
}

/// term bump 与 `GetMapper` 的组合：先抬到下一文件边界，再按映射反推统计
/// （C++ 的两个动作顺序与结果）。16 已对齐 ⇒ 20。
#[test]
fn append_term_bump_keeps_get_mapper_stat_shape() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l210_append_bump", 1).unwrap();
    let io = mem();
    io.switch_manifest(
      &tbl,
      &record(
        MAX_FP,
        &[
          INVALID_VALUE,
          encode_file_page_id(5),
          INVALID_VALUE,
          encode_file_page_id(12),
        ],
        &BranchManifestMetadata::new("main", 0),
        NULL_PAGE,
      ),
    )
    .await
    .unwrap();

    let mut store = store(io, AllocMode::Append);
    store.prepare_attach("main", 3).unwrap();
    store.reopen(&tbl).await.unwrap();
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(
      mapper.max_fp_id(),
      MAX_FP + (1 << SHIFT),
      "term 变化抬到下一文件边界（C++ replayer.cpp:358-361）"
    );
    assert_eq!(mapper.min_file_id(), 1, "bump 不改变映射统计");
    assert_eq!(mapper.space_size(SHIFT), 12, "(20 - 4) - 1 个空文件 × 4");
  });
}

/// C++ `GetMapper` 的 pooled 分支：空闲池 = `[0, max_fp_id)` 去掉 using 集合
/// （无 `file_ranges` 时全量回填），分配顺序为从最高空闲页降序
/// （`free_ids` 升序构建 + `pop_back`）。
#[test]
fn pooled_reopen_refills_exact_free_page_complement() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l210_pooled", 1).unwrap();
    let io = mem();
    io.switch_manifest(
      &tbl,
      &record(
        MAX_FP,
        &[
          INVALID_VALUE,
          encode_file_page_id(5),
          INVALID_VALUE,
          encode_file_page_id(12),
        ],
        &BranchManifestMetadata::new("main", 0),
        NULL_PAGE,
      ),
    )
    .await
    .unwrap();

    let mut store = store(io, AllocMode::Pooled);
    store.prepare_attach("main", 0).unwrap();
    store.reopen(&tbl).await.unwrap();
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.max_fp_id(), MAX_FP);
    assert_eq!(mapper.pooled_free_len(), MAX_FP - 2, "[0,16) 去掉 5 / 12");
    // 空洞 [0,5) / [6,12) / [13,16)：只记区间、不逐页展开。
    assert_eq!(mapper.pooled_free_range_cnt(), 3);

    let mut probe = mapper.clone();
    let got: Vec<u64> = (0..14)
      .map(|_| {
        let page_id = probe.allocate_page();
        probe.file_page_id(page_id).unwrap()
      })
      .collect();
    assert_eq!(
      got,
      vec![15, 14, 13, 11, 10, 9, 8, 7, 6, 4, 3, 2, 1, 0],
      "必须与 C++ free_ids pop_back 的降序一致"
    );
    assert_eq!(probe.max_fp_id(), MAX_FP, "池耗尽前不推进水位");
    let page_id = probe.allocate_page();
    assert_eq!(probe.file_page_id(page_id).unwrap(), MAX_FP);
    assert_eq!(probe.max_fp_id(), MAX_FP + 1);
  });
}

/// C++ `FileIdInBranch` 过滤：`file_ranges` 非空时，父 branch 名下的空洞不得
/// 回填本分支的空闲池（否则续写会按 file id 写进父 branch 的命名空间）。
#[test]
fn pooled_branch_reopen_only_refills_own_file_ranges() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l210_branch", 1).unwrap();
    let io = mem();
    let metadata = BranchManifestMetadata {
      branch_name: "feature".into(),
      term: 0,
      file_range_li: vec![
        // main 拥有 file 0；feature 拥有 file 1..=3（page 4..16）。
        BranchFileRange {
          branch_name: "main".into(),
          term: 0,
          max_file_id: 0,
          max_segment_file_id: 0,
        },
        BranchFileRange {
          branch_name: "feature".into(),
          term: 0,
          max_file_id: 3,
          max_segment_file_id: 3,
        },
      ],
    };
    let bytes = record(
      MAX_FP,
      &[
        INVALID_VALUE,
        encode_file_page_id(5),
        INVALID_VALUE,
        encode_file_page_id(12),
      ],
      &metadata,
      NULL_PAGE,
    );
    io.write_manifest_file(&tbl, &manifest_file_name("feature", 0), &bytes)
      .await
      .unwrap();

    let mut store = store(io, AllocMode::Pooled);
    store.prepare_attach("feature", 0).unwrap();
    store.reopen(&tbl).await.unwrap();
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.max_fp_id(), MAX_FP);
    // feature 区间 4..16 去掉 5 / 12：10 页，文件 0 的 4 页不属于本分支。
    assert_eq!(mapper.pooled_free_len(), 10);
    assert_eq!(mapper.pooled_free_range_cnt(), 3);

    let mut probe = mapper.clone();
    let got: Vec<u64> = (0..10)
      .map(|_| {
        let page_id = probe.allocate_page();
        probe.file_page_id(page_id).unwrap()
      })
      .collect();
    assert_eq!(got, vec![15, 14, 13, 11, 10, 9, 8, 7, 6, 4]);
    assert!(
      got.iter().all(|file_page_id| *file_page_id >= 1 << SHIFT),
      "file 0 属 main，feature 不得复用: {got:?}"
    );
  });
}

/// C++ `CreateBranch` 写下的 branch manifest 形状：metadata 的 `file_ranges`
/// 只有父分支链、**没有本分支区间**（`background_write.cpp:634-638`），
/// `max_fp_id = (父 CurrentFileId + 1) << shift`。此时 `term_range_start` 为
/// `None`（没有本 branch 区间），复用下界是 0——唯一挡住父页回填的就是
/// `FileIdInBranch`：C++ 对 `[0, max_fp_id)` 全部判为「非本分支」⇒ 空闲池为空。
#[test]
fn cpp_create_branch_manifest_has_no_reusable_parent_pages() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l210_created_branch", 1).unwrap();
    let io = mem();
    // 父分支只写了 file 0（page 0..4）：CreateBranch 的 branch manifest
    // 继承父映射、水位 = file 1 起点 = 4，区间链只有 main max_file=0。
    let metadata = BranchManifestMetadata {
      branch_name: "feature".into(),
      term: 0,
      file_range_li: vec![BranchFileRange {
        branch_name: "main".into(),
        term: 0,
        max_file_id: 0,
        max_segment_file_id: 0,
      }],
    };
    let bytes = record(
      1 << SHIFT,
      // file 0 内留一个洞（page 1）：父映射在 COW 后可能就是这样。
      &[
        encode_file_page_id(0),
        INVALID_VALUE,
        encode_file_page_id(2),
        encode_file_page_id(3),
      ],
      &metadata,
      NULL_PAGE,
    );
    io.write_manifest_file(&tbl, &manifest_file_name("feature", 0), &bytes)
      .await
      .unwrap();

    let mut store = store(io, AllocMode::Pooled);
    store.prepare_attach("feature", 0).unwrap();
    store.reopen(&tbl).await.unwrap();
    let mapper = store.mapper(&tbl).unwrap();
    assert_eq!(mapper.max_fp_id(), 1 << SHIFT);
    assert_eq!(
      mapper.pooled_free_len(),
      0,
      "父分支 file 0 的页不属于 feature，一个都不得回池"
    );
    assert_eq!(mapper.pooled_free_range_cnt(), 0);

    // 续写从水位（file 1，feature 自己的新文件）开始，而不是复用父页。
    let mut probe = mapper.clone();
    let page_id = probe.allocate_page();
    assert_eq!(probe.file_page_id(page_id).unwrap(), 1 << SHIFT);
    assert_eq!(probe.max_fp_id(), (1 << SHIFT) + 1);
  });
}

/// 端到端：C++ 形状的 pooled manifest（含删除后的映射空洞）→ Rust reopen
/// 续写 → 再次 reopen 读回。续写分配不得与仍被引用的物理页撞号。
#[test]
fn cpp_shaped_pooled_manifest_continuation_roundtrip() {
  pollster::block_on(async {
    let tbl = TableIdent::new("l210_roundtrip", 1).unwrap();
    let io = mem();
    let mut writer = store(io, AllocMode::Pooled);
    let value = vec![7u8; 200];
    let keys = ["k00", "k01", "k02", "k03", "k04", "k05", "k06", "k07"];
    for (i, key) in keys.iter().enumerate() {
      writer
        .put(&tbl, &[entry(key, &value, i as u64 + 1)])
        .await
        .unwrap();
    }
    // 删除中间 key：映射出现空洞，水位不回落——与 C++ reopen 前的盘面同形。
    writer.delete(&tbl, b"k02", 10).await.unwrap();
    writer.delete(&tbl, b"k05", 11).await.unwrap();

    let mapping_value_li: Vec<u64> = writer.mapper(&tbl).unwrap().tbl().iter_value_li().collect();
    let max_fp_id = writer.mapper(&tbl).unwrap().max_fp_id();
    let root_id = writer.root_id(&tbl).unwrap();
    let used = live_pages(&writer, &tbl);
    assert!(used.len() >= 2);

    // pre-range-chain legacy manifest 的 branch metadata：main / term 0 / 空区间链
    // （C++ 82b774c 在分配时经 `SetBranchFileIdTerm` 写入 `(main, 0)` 区间，空链
    // 是 pre-range-chain 遗留形状，不是当前 C++ Local 的首个 main manifest）。
    // 用 `ManifestBuilder::Snapshot` 同布局重写：只保留 snapshot 一条 record。
    let io = writer.into_io();
    io.switch_manifest(
      &tbl,
      &record(
        max_fp_id,
        &mapping_value_li,
        &BranchManifestMetadata::new("main", 0),
        root_id,
      ),
    )
    .await
    .unwrap();

    let mut reopened = store(io, AllocMode::Pooled);
    reopened.prepare_attach("main", 0).unwrap();
    reopened.reopen(&tbl).await.unwrap();
    {
      let mapper = reopened.mapper(&tbl).unwrap();
      assert_eq!(mapper.max_fp_id(), max_fp_id);
      assert_eq!(
        mapper.pooled_free_len(),
        max_fp_id - used.len() as u64,
        "空闲池必须是 [0, max) 对活跃页的精确补集"
      );
    }
    let kept = ["k00", "k01", "k03", "k04", "k06", "k07"];
    for key in kept {
      assert_eq!(
        reopened.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(value.clone()),
        "reopen 后必须按 C++ manifest 读回 {key}"
      );
    }
    assert!(reopened.get(&tbl, b"k02").await.unwrap().is_none());

    // 续写：新分配只能来自补集（池）或水位之后，仍被引用的页不得被复用。
    for (i, key) in ["n00", "n01", "n02"].iter().enumerate() {
      reopened
        .put(&tbl, &[entry(key, &value, 20 + i as u64)])
        .await
        .unwrap();
    }
    let physical: Vec<u64> = reopened
      .mapper(&tbl)
      .unwrap()
      .tbl()
      .iter_value_li()
      .filter(|value| is_file_page_id(*value))
      .map(decode_id)
      .collect();
    let unique: BTreeSet<u64> = physical.iter().copied().collect();
    assert_eq!(unique.len(), physical.len(), "物理页不得被两个逻辑页引用");
    assert!(reopened.mapper(&tbl).unwrap().max_fp_id() >= max_fp_id);

    // 再 reopen 一次：续写结果必须可恢复。
    let io = reopened.into_io();
    let mut again = store(io, AllocMode::Pooled);
    again.prepare_attach("main", 0).unwrap();
    again.reopen(&tbl).await.unwrap();
    for key in kept {
      assert_eq!(
        again.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(value.clone()),
        "二次 reopen 后继承数据丢失: {key}"
      );
    }
    for key in ["n00", "n01", "n02"] {
      assert_eq!(
        again.get(&tbl, key.as_bytes()).await.unwrap(),
        Some(value.clone()),
        "二次 reopen 后续写数据丢失: {key}"
      );
    }
    assert!(again.get(&tbl, b"k02").await.unwrap().is_none());
  });
}
