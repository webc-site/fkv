//! 写路径 COW：整批只路由一次，data 页写完后自底向上重建 index 页；子项集合
//! 未变的 index 页（原位覆盖、未分裂/未删空）整条祖先链都不落盘，同一 index 页
//! 每批只重建一次（issue 49）。
//!
//! `ttl` 子模块提供 TTL 索引 key 编码与写入批次生成。

mod batch;
mod data;
mod index;
mod overflow;
mod rebalance;
pub mod ttl;

pub use batch::{PageLayout, WriteArg, write_batch};
pub use ttl::{PreparedWriteBatch, is_expired, parse_ttl_key, prepare_write_batch, ttl_key};

#[cfg(test)]
mod unit_tests {
  use std::sync::atomic::Ordering;

  use fkv_btree::{
    BatchEntry, MergeOption, MergeOutput, OverflowSlot, comparator::find_shortest_separator,
    merge_data_pages_with_options,
  };
  use fkv_error::KvError;
  use fkv_io::PageIo;
  use fkv_io_mem::MemStore;
  use fkv_page::{
    CompressionType, DataPageBuilder, DataPageIter, EntrySpec, IndexPageBuilder, content_len,
    header::OVERFLOW_HEADER_SIZE, restart_num,
  };
  use fkv_types::{NULL_PAGE, PageId, TableIdent};
  use gxhash::HashMap;

  use super::{PageLayout, WriteArg};
  use crate::{batch::*, data::*, index::*, overflow::*, rebalance::*};

  /// `end_cap == 0` 的极端页配置：末页写不出任何字节，旧循环会原地打转
  /// 并无界分配页，必须在入口拒绝且不分配任何页。
  #[test]
  fn overflow_chain_rejects_zero_end_capacity() {
    use fkv_mapping::{AllocMode, PageMapper};

    let pointers = 16u8;
    let page_size = OVERFLOW_HEADER_SIZE + 1 + 4 * usize::from(pointers);
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut pending = PendingWrite {
      page_li: Vec::new(),
      slot_map: HashMap::default(),
      bytes: 0,
    };
    // 超过一批容量（page_cap * group_max），才会走到末页指针分支。
    let value = vec![b'x'; 2048];
    let err = pollster::block_on(build_overflow_chain(
      &MemStore::new(page_size),
      &TableIdent::new("demo", 1).unwrap(),
      &mut mapper,
      &value,
      PageLayout::new(page_size, 4, 4, None, pointers),
      &mut pending,
    ))
    .unwrap_err();
    assert!(matches!(err, KvError::InvalidArgs));
    assert_eq!(mapper.tbl().len(), 0, "校验必须先于任何页分配");
    assert!(pending.page_li.is_empty());
  }

  /// 构造 index 页的 child_li：`child_li` 首项是 leftmost（key 空），其余为 `keys`。
  fn child_li(keys: &[&str], page_id_base: u32) -> Children {
    let mut out = Children::with_capacity(keys.len() + 1, keys.iter().map(|key| key.len()).sum());
    out.push(b"", page_id_base).unwrap();
    for (idx, key) in keys.iter().enumerate() {
      out
        .push(key.as_bytes(), page_id_base + 1 + idx as u32)
        .unwrap();
    }
    out
  }

  /// 单个子项的变更列表（data 页未分裂时的重建结果）。
  fn only_child(key: &[u8], page_id: PageId) -> Children {
    let mut out = Children::with_capacity(1, key.len());
    out.push(key, page_id).unwrap();
    out
  }

  /// 测试用：`child_li[start..end]` 的独立副本。
  fn slice_children(child_li: &Children, start: usize, end: usize) -> Children {
    let mut out = Children::with_capacity(end - start, child_li.keys.len());
    for pos in start..end {
      let (key, page_id) = child_li.child(pos).unwrap();
      out.push(key, page_id).unwrap();
    }
    out
  }

  /// 测试用：把第 `idx` 个子项替换为 `(key, page_id)`（重建键区）。
  fn set_child(child_li: &mut Children, idx: usize, key: &[u8], page_id: PageId) {
    let mut out = Children::with_capacity(child_li.len(), child_li.keys.len() + key.len());
    for pos in 0..child_li.len() {
      if pos == idx {
        out.push(key, page_id).unwrap();
      } else {
        let (old_key, old_page_id) = child_li.child(pos).unwrap();
        out.push(old_key, old_page_id).unwrap();
      }
    }
    *child_li = out;
  }

  /// 测试用：截短子项列表（键区可保留冗余字节）。
  fn truncate_children(child_li: &mut Children, len: usize) {
    child_li.entries.truncate(len);
  }

  /// 测试基准：按原 `replace_child` 语义逐个 splice，最后清空首项键。
  fn splice_reference(old: &Children, changes: &[(usize, Children)]) -> Children {
    let mut out = Children::default();
    let mut change_idx = 0usize;
    for pos in 0..old.len() {
      if let Some((_, new)) = changes.get(change_idx).filter(|(idx, _)| *idx == pos) {
        for child_idx in 0..new.len() {
          let (key, page_id) = new.child(child_idx).unwrap();
          out.push(key, page_id).unwrap();
        }
        change_idx += 1;
      } else {
        let (key, page_id) = old.child(pos).unwrap();
        out.push(key, page_id).unwrap();
      }
    }
    if let Some(first) = out.entries.first_mut() {
      first.len = 0;
    }
    out
  }

  /// 子项定位的二分实现必须与「从 `from` 起线性右推」逐位等价。
  ///
  /// `RouteCursor::open`（按批首键下降）与 `page_for`（批内推进）都改用
  /// [`last_child_le_from`] 定位子项；这里对全部 `from` 与各类边界键（空键、
  /// 小于首键、恰好等于分隔键、介于两键之间、大于末键）逐位比对线性实现，
  /// 保证定位语义与改动前完全一致。
  #[test]
  fn last_child_le_matches_linear_scan() {
    let children = child_li(&["k0001", "k0003", "k0005", "k0007", "k0009"], 100);
    // 线性参考实现：与改动前 `page_for` 的右推循环逐字同构。
    let linear = |key: &[u8], from: usize| -> usize {
      let mut idx = from;
      while let Some((next_key, _)) = children.child(idx + 1) {
        if next_key.is_empty() || next_key > key {
          break;
        }
        idx += 1;
      }
      idx
    };
    let probes: Vec<Vec<u8>> = vec![
      b"".to_vec(),
      b"a".to_vec(),
      b"k0001".to_vec(),
      b"k00015".to_vec(),
      b"k0002".to_vec(),
      b"k0003".to_vec(),
      b"k0004".to_vec(),
      b"k0009".to_vec(),
      b"k0010".to_vec(),
      b"z".to_vec(),
    ];
    for from in 0..children.len() {
      for key in &probes {
        assert_eq!(
          last_child_le_from(&children, key, from).unwrap(),
          linear(key, from),
          "from={from} key={key:?}"
        );
      }
    }
    // `from = 0` 的全量定位同样一致（`open` 的下降入口）。
    for key in &probes {
      assert_eq!(
        last_child_le(&children, key).unwrap(),
        linear(key, 0),
        "key={key:?}"
      );
    }
    // 单子项 / 空子项列表不 panic（越界 `from` 由调用方保证，夹取后取到同样的
    // `Corrupted` 语义）。
    let single = child_li(&[], 7);
    assert_eq!(last_child_le(&single, b"k").unwrap(), 0);
    let empty = Children::default();
    assert_eq!(last_child_le_from(&empty, b"k", 0).unwrap(), 0);
  }

  /// 不复制整表的「子项集合未变」判定必须与旧实现（clone + splice + 整表 `==`）
  /// 完全等价，覆盖同值、改页、改键、删项与插入/分裂。
  #[test]
  fn children_unchanged_matches_splice_compare() {
    let old = child_li(&["k0001", "k0002", "k0003", "k0004", "k0005"], 100);
    let split = {
      let mut children = only_child(b"k0002", 102);
      children.push(b"k0002b", 999).unwrap();
      children
    };
    let cases = vec![
      vec![],
      vec![(2usize, only_child(b"k0002", 103))],
      vec![(2, only_child(b"k0002", 999))],
      vec![(2, only_child(b"k0002x", 103))],
      vec![(0, only_child(b"", 100))],
      vec![(0, only_child(b"", 101))],
      vec![(0, Children::default())],
      vec![
        (1, only_child(b"k0001", 102)),
        (3, only_child(b"k0003", 104)),
      ],
      vec![(4, only_child(b"k0005x", 105))],
      vec![(2, split)],
      vec![(1, Children::default()), (3, only_child(b"k0003", 999))],
    ];
    for changes in cases {
      let spliced = splice_reference(&old, &changes);
      assert_eq!(
        children_unchanged(&old, &changes),
        spliced == old,
        "changes={changes:?}"
      );
    }
  }

  fn index_layout(page_size: usize, restart_interval: u16, leaf: bool) -> IndexLayout<'static> {
    IndexLayout {
      page: PageLayout::new(
        page_size,
        // 测试里两页间隔同值：index 布局只消费 index 侧。
        restart_interval,
        restart_interval,
        None,
        16,
      ),
      leaf,
    }
  }

  /// fmt-18：index 页 restart 密度只由 `index_restart_interval` 决定，
  /// data 侧改动不得影响 index 页字节（对齐 C++ 两独立旋钮）。
  #[test]
  fn index_page_uses_index_restart_interval_only() {
    let page_size = 4096;
    let keys: Vec<String> = (0..12).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
    let child_li = child_li(&key_ref_li, 100);
    let layout_of = |data_restart_interval: u16, index_restart_interval: u16| IndexLayout {
      page: PageLayout::new(
        page_size,
        data_restart_interval,
        index_restart_interval,
        None,
        16,
      ),
      leaf: true,
    };
    let build = |layout: IndexLayout<'static>| {
      let page_li = build_index_page_li(b"", &child_li, layout).unwrap();
      page_li.into_iter().next().unwrap().1
    };

    let base = build(layout_of(16, 16));
    // data 侧改动：index 页字节必须逐字节不变。
    assert_eq!(base, build(layout_of(2, 16)));
    // index 侧改动：restart 点密度变化，字节随之改变，语义（child_li 序列）不变。
    let dense = build(layout_of(16, 4));
    assert_ne!(base, dense);
    assert_eq!(restart_num(&base).unwrap(), 1);
    assert_eq!(restart_num(&dense).unwrap(), 3);
    assert_eq!(read_index_children(&base).unwrap().0, child_li);
    assert_eq!(read_index_children(&dense).unwrap().0, child_li);
  }

  /// fmt-18：data 页只受 `data_restart_interval` 影响，index 侧改动不得改变 data 页字节。
  #[test]
  fn data_page_ignores_index_restart_interval() {
    use fkv_io_mem::MemStore;
    use fkv_mapping::{AllocMode, PageMapper};
    use fkv_types::TableIdent;

    pollster::block_on(async {
      const PAGE_SIZE: usize = 512;
      let batch_li: Vec<BatchEntry> = (0..40)
        .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
        .collect();
      let entry_li: Vec<&BatchEntry> = batch_li.iter().collect();

      let dump = async |index_restart_interval: u16| -> Vec<Vec<u8>> {
        let store = MemStore::new(PAGE_SIZE);
        let tbl = TableIdent::new("t", 1).unwrap();
        let mut mapper = PageMapper::new(AllocMode::Append, 0);
        let layout = PageLayout::new(PAGE_SIZE, 4, index_restart_interval, None, 16);
        let mut pending = PendingWrite::default();
        let child_li =
          write_new_data_page(&store, &tbl, &mut mapper, &entry_li, layout, &mut pending)
            .await
            .unwrap();
        // 首写页在批末统一落盘（perf-02）：测试需先 flush 再读回。
        pending.flush(&store, &tbl).await.unwrap();
        let mut buf = vec![0u8; PAGE_SIZE];
        let mut out_li = Vec::with_capacity(child_li.len());
        for idx in 0..child_li.len() {
          let file_page_id = mapper.file_page_id(child_li.page_id(idx).unwrap()).unwrap();
          store.read_page(&tbl, file_page_id, &mut buf).await.unwrap();
          out_li.push(buf.clone());
        }
        out_li
      };

      let wide = dump(16).await;
      let narrow = dump(2).await;
      assert!(wide.len() > 1, "测试数据须产生多个 data 页");
      assert_eq!(wide, narrow, "index 间隔不得改变 data 页字节");
    });
  }

  /// 单页重建：region 直拷产物必须与 `build_index_page_li` 逐条重编码**逐字节一致**。
  /// 覆盖：全不变 / 改 page_id / 改分隔键 / child_li 变短变长。
  #[test]
  fn region_reuse_matches_entry_rebuild() {
    let page_size = 4096;
    let ri = 4u16;
    let layout = index_layout(page_size, ri, true);
    let keys: Vec<String> = (0..40).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
    let old_child_li = child_li(&key_ref_li, 100);
    let old_pages = build_index_page_li(b"", &old_child_li, layout).unwrap();
    assert_eq!(old_pages.len(), 1, "测试数据须落在单页");
    let old_page = &old_pages[0].1;

    // 逐条重编码基准（同一 child_li → 单页）。
    let expect_single = |child_li: &Children| -> Vec<u8> {
      let pages = build_index_page_li(b"", child_li, layout).unwrap();
      assert_eq!(pages.len(), 1, "基准重建须仍在单页");
      pages.into_iter().next().unwrap().1
    };

    // 1) 完全不变：直拷产物 == 旧页字节（且命中快路径）。
    let same = build_index_page_region(&old_child_li, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(&same, old_page);

    // 2) 改一个 child 的 page_id（key 不变）。
    let mut changed = old_child_li.clone();
    set_child(&mut changed, 10, b"k0009", 9999);
    let fast = build_index_page_region(&changed, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&changed));
    assert_eq!(read_index_children(&fast).unwrap().0, changed);

    // 3) 改一个 child 的分隔键（key 变化）。
    let mut changed = old_child_li.clone();
    set_child(&mut changed, 5, b"k0004x", 5555);
    let fast = build_index_page_region(&changed, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&changed));

    // 4) child_li 变短（尾区段比旧页对应区段短）与变长。
    let mut shorter = old_child_li.clone();
    truncate_children(&mut shorter, 11);
    let fast = build_index_page_region(&shorter, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&shorter));

    let mut longer = old_child_li.clone();
    longer.push(b"k0100", 7777).unwrap();
    let fast = build_index_page_region(&longer, &old_child_li, old_page, layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(fast, expect_single(&longer));

    // 5) 旧页 leaf 与当前布局不符 → 不适用。
    assert!(
      build_index_page_region(
        &old_child_li,
        &old_child_li,
        old_page,
        index_layout(page_size, ri, false)
      )
      .unwrap()
      .is_none()
    );
  }

  /// 旧页 `restart_interval` 与当前配置不一致时必须放弃直拷（否则 region 错位串页）。
  #[test]
  fn region_reuse_rejects_restart_interval_mismatch() {
    let page_size = 4096;
    let keys: Vec<String> = (0..6).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
    let child_li = child_li(&key_ref_li, 100);

    // 旧页按 ri=3 切 region：region 数 2，与 ri=4 的 ceil(6/4)=2 相同（只靠计数挡不住）。
    let old_layout = index_layout(page_size, 3, true);
    let old_page = &build_index_page_li(b"", &child_li, old_layout).unwrap()[0].1;
    assert!(
      build_index_page_region(
        &child_li,
        &child_li,
        old_page,
        index_layout(page_size, 4, true)
      )
      .unwrap()
      .is_none()
    );
    assert!(
      build_index_page_region(
        &child_li,
        &child_li,
        old_page,
        index_layout(page_size, 8, true)
      )
      .unwrap()
      .is_none()
    );
    // 同配置则命中，且与逐条重编码一致。
    let fast = build_index_page_region(&child_li, &child_li, old_page, old_layout)
      .unwrap()
      .expect("应命中 region 快路径");
    assert_eq!(&fast, old_page);
  }

  /// `build_index_pages` 在旧页可用时必须走 region 直拷，且产物与逐条重编码一致。
  #[test]
  fn build_index_pages_prefers_region_reuse() {
    use fkv_mapping::{AllocMode, PageMapper};

    let page_size = 4096;
    let layout = index_layout(page_size, 4, true);
    let keys: Vec<String> = (0..30).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = keys.iter().map(String::as_str).collect();
    let old_child_li = child_li(&key_ref_li, 100);
    let old_page = build_index_page_li(b"", &old_child_li, layout)
      .unwrap()
      .remove(0)
      .1;
    let mut new_children = old_child_li.clone();
    set_child(&mut new_children, 7, b"k0006", 4242);
    let changes = [(7usize, only_child(b"k0006", 4242))];

    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let old_page_id = mapper.allocate_page();
    let mut pending = PendingWrite::default();

    let hits_before = REGION_REUSE_HITS.load(Ordering::Relaxed);
    let out_li = build_index_pages(
      &mut mapper,
      &mut pending,
      IndexRebuild {
        page_id: old_page_id,
        first_key: b"",
        old_child_li: &old_child_li,
        old_page: &old_page,
        changes: &changes,
      },
      layout,
    )
    .unwrap();
    assert!(
      REGION_REUSE_HITS.load(Ordering::Relaxed) > hits_before,
      "应命中 region 直拷快路径"
    );
    assert_eq!(out_li.len(), 1);
    assert_eq!(out_li.page_id(0), Some(old_page_id));
    let written = mapper.file_page_id(old_page_id).unwrap();
    // 页内容已进批内缓冲（延迟落盘），不直接写 store。
    assert_eq!(
      pending.get(written).expect("索引页应进入批内缓冲"),
      build_index_page_li(b"", &new_children, layout).unwrap()[0].1
    );
  }

  /// 真实写路径：第二批只改少量 data 页时，leaf index 重建必须命中 region 直拷。
  #[test]
  fn write_batch_uses_region_reuse() {
    use fkv_io_mem::MemStore;
    use fkv_mapping::{AllocMode, PageMapper};
    use fkv_types::TableIdent;

    pollster::block_on(async {
      let page_size = 512;
      let store = MemStore::new(page_size);
      let tbl = TableIdent::new("t", 2).unwrap();
      let mut mapper = PageMapper::new(AllocMode::Append, 0);
      let batch_li: Vec<BatchEntry> = (0..200)
        .map(|idx| BatchEntry::upsert(format!("k{idx:04}").into_bytes(), vec![b'v'; 8], 1))
        .collect();
      let root = write_batch(
        &store,
        WriteArg {
          tbl: &tbl,
          root_id: NULL_PAGE,
          mapper: &mut mapper,
          batch_li: &batch_li,
          layout: PageLayout::new(page_size, 4, 4, None, 16),
          segment_mapper: None,
          segment_size: 0,
          ttl_delete_sink: None,
        },
      )
      .await
      .unwrap();

      // 插入一批更大的 key：末位 data 页分裂 → leaf index 子项集合改变，触发重建；
      // 其余子项不变，故首个 region 可直接直拷。
      let batch2: Vec<BatchEntry> = (0..60)
        .map(|idx| BatchEntry::upsert(format!("n{idx:04}").into_bytes(), vec![b'w'; 8], 2))
        .collect();
      let hits_before = REGION_REUSE_HITS.load(Ordering::Relaxed);
      let _root2 = write_batch(
        &store,
        WriteArg {
          tbl: &tbl,
          root_id: root,
          mapper: &mut mapper,
          batch_li: &batch2,
          layout: PageLayout::new(page_size, 4, 4, None, 16),
          segment_mapper: None,
          segment_size: 0,
          ttl_delete_sink: None,
        },
      )
      .await
      .unwrap();
      assert!(
        REGION_REUSE_HITS.load(Ordering::Relaxed) > hits_before,
        "leaf index 重建应命中 region 直拷"
      );
    });
  }

  /// 最短分隔键必须落在 `(前页末键, 本页首键]`，才不破坏 index 路由。
  #[test]
  fn shortest_separator_stays_between_keys() {
    for (left, right) in [
      (&b"abc"[..], &b"abd"[..]),
      (&b"abc"[..], &b"abcd"[..]),
      (&b"a"[..], &b"b"[..]),
      (&b"key0100"[..], &b"key0200"[..]),
    ] {
      let sep = find_shortest_separator(left, right);
      assert!(left < sep, "left={left:?} sep={sep:?}");
      assert!(sep <= right, "sep={sep:?} right={right:?}");
    }
  }

  fn page_of(keys: &[&str], value: &[u8], page_size: usize) -> (Vec<u8>, usize) {
    let mut builder = DataPageBuilder::new(page_size, 8).unwrap();
    let mut added = 0;
    for key in keys {
      if !builder
        .add(EntrySpec {
          key: key.as_bytes(),
          value,
          ts: 1,
          expire_ts: 0,
          overflow: false,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap()
      {
        break;
      }
      added += 1;
    }
    (builder.finish().unwrap(), added)
  }

  /// 用给定 child_li 构建单页（全部须放得下），返回页字节。
  fn index_page_of(child_li: &Children, layout: IndexLayout<'static>) -> Vec<u8> {
    let mut builder = IndexPageBuilder::new(
      layout.page.data.page_size,
      layout.page.index_restart_interval,
      layout.leaf,
    )
    .unwrap();
    assert!(builder.add(b"", child_li.page_id(0).unwrap()).unwrap());
    for idx in 1..child_li.len() {
      let (key, page_id) = child_li.child(idx).unwrap();
      assert!(builder.add(key, page_id).unwrap());
    }
    builder.finish().unwrap()
  }

  /// 把多页索引的 child_li 顺序拉平：首页 leftmost 键为空，其后各页 leftmost
  /// 键即该页分隔键。
  fn flatten_index_children(page_li: &[(Vec<u8>, Vec<u8>)]) -> Children {
    let mut out = Children::default();
    for (idx, (key, page)) in page_li.iter().enumerate() {
      let child_li = read_index_children(page).unwrap().0;
      let leftmost = child_li.page_id(0).unwrap();
      let leftmost_key: &[u8] = if idx == 0 { b"" } else { key };
      out.push(leftmost_key, leftmost).unwrap();
      for pos in 1..child_li.len() {
        let (child_key, page_id) = child_li.child(pos).unwrap();
        out.push(child_key, page_id).unwrap();
      }
    }
    out
  }

  /// sto-67：索引页尾页不足 1/4、前页超 3/4 且 restart 数 > 1 时，按 restart
  /// region 把前页尾部迁到尾页，消除稀疏尾页；child_li 顺序不变。
  #[test]
  fn index_page_quarter_redistribution_balances_tail() {
    let page_size = 512;
    let restart_interval = 4;
    let layout = index_layout(page_size, restart_interval, true);
    let key_li: Vec<String> = (0..200).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = key_li.iter().map(String::as_str).collect();
    let children_all = child_li(&key_ref_li, 100);

    // 复刻无重分配路径：按「放不下就开新页」切分，得到前页 + 稀疏尾页。
    let mut builder = IndexPageBuilder::new(page_size, restart_interval, true).unwrap();
    assert!(builder.add(b"", children_all.page_id(0).unwrap()).unwrap());
    let mut split = 0;
    for pos in 1..children_all.len() {
      let (key, page_id) = children_all.child(pos).unwrap();
      if !builder.add(key, page_id).unwrap() {
        split = pos;
        break;
      }
    }
    assert!(split > 1, "fixture 需先填满前页");
    // 尾页只留 leftmost + 1 条 entry，确保 < 1/4。
    let child_li = slice_children(&children_all, 0, split + 2);
    let prev = index_page_of(&slice_children(&child_li, 0, split), layout);
    let cur = index_page_of(&slice_children(&child_li, split, child_li.len()), layout);
    let prev_len = usize::from(content_len(&prev).unwrap());
    let cur_len = usize::from(content_len(&cur).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(cur_len < page_size / 4, "cur={cur_len}");
    assert!(restart_num(&prev).unwrap() > 1);

    let mut page_li = vec![
      (b"".to_vec(), prev),
      (child_li.key(split).unwrap().to_vec(), cur),
    ];
    let old_cur_key = page_li[1].0.clone();
    assert!(rebalance_index_page_li(&mut page_li, page_size, restart_interval, true).unwrap());
    // 尾页不再 < 1/4（前页同样落在阈值内）。
    for (_, page) in &page_li {
      let len = usize::from(content_len(page).unwrap());
      assert!(
        len >= page_size / 4 && len <= page_size * 3 / 4,
        "len={len}"
      );
    }
    // child_li 顺序不变：各页 leftmost + entry 依次拼接（页首 leftmost 键即页分隔键）。
    let out = flatten_index_children(&page_li);
    assert_eq!(out, child_li);
    // 尾页首键已改为迁移入口的键（不再是原尾页首键）。
    assert_ne!(page_li[1].0, old_cur_key);
  }

  /// 端到端：`build_index_page_li` 自动重分配，尾页不再稀疏，页数不变。
  #[test]
  fn build_index_page_li_rebalances_sparse_tail() {
    let page_size = 512;
    let layout = index_layout(page_size, 4, true);
    let key_li: Vec<String> = (0..130).map(|idx| format!("k{idx:04}")).collect();
    let key_ref_li: Vec<&str> = key_li.iter().map(String::as_str).collect();
    let child_li = child_li(&key_ref_li, 100);

    let page_li = build_index_page_li(b"", &child_li, layout).unwrap();
    assert!(page_li.len() >= 2);
    let last_len = usize::from(content_len(&page_li.last().unwrap().1).unwrap());
    assert!(last_len >= page_size / 4, "last={last_len}");
    let out = flatten_index_children(&page_li);
    assert_eq!(out, child_li);
  }

  /// 前页 > 3/4、后页 < 1/4 时，重分配后两页都落在 [1/4, 3/4]。
  #[test]
  fn quarter_redistribution_balances_pages() {
    let page_size = 256;
    let value = vec![b'v'; 8];
    let key_li: Vec<String> = (0..24).map(|index| format!("k{index:02}")).collect();
    let key_ref_li: Vec<&str> = key_li.iter().map(String::as_str).collect();
    let (prev, prev_cnt) = page_of(&key_ref_li, &value, page_size);
    assert!(prev_cnt >= 2, "prev page must hold multiple entry_li");
    let last_key = format!("k{prev_cnt:02}");
    let (cur, cur_cnt) = page_of(&[last_key.as_str()], &value, page_size);
    assert_eq!(cur_cnt, 1);

    let prev_len = usize::from(content_len(&prev).unwrap());
    let cur_len = usize::from(content_len(&cur).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(cur_len < page_size / 4, "cur={cur_len}");

    let mut page_li = vec![prev, cur];
    let mut redistributed_li = Vec::new();
    rebalance_data_page_li(&mut page_li, page_size, 8, &mut [], &mut redistributed_li).unwrap();
    // 后页（新页）首键被改写，须标记为改用真实首键；前页未标记。
    assert_eq!(redistributed_li, vec![false, true]);
    assert_eq!(page_li.len(), 2);
    for page in &page_li {
      let len = usize::from(content_len(page).unwrap());
      assert!(
        len >= page_size / 4 && len <= page_size * 3 / 4,
        "len={len}"
      );
    }
    // 全部 entry 且顺序不变。
    let mut keys = Vec::new();
    for page in &page_li {
      let mut iter = DataPageIter::new(page).unwrap();
      while iter.advance().unwrap() {
        keys.push(iter.key().to_vec());
      }
    }
    let expected: Vec<Vec<u8>> = (0..=prev_cnt)
      .map(|index| format!("k{index:02}").into_bytes())
      .collect();
    assert_eq!(keys, expected);
  }

  /// 被四分页重分配改写过首键的新页：分隔键用真实首键（对齐 C++
  /// `page_key.assign(iter.Key())`）；未重分配的页仍用最短分隔键。
  #[test]
  fn redistributed_page_uses_real_first_key() {
    let first_sep = b"parent".to_vec();
    let (page_a, _) = page_of(&["k00", "k01", "k02"], b"v", 256);
    let (page_b, _) = page_of(&["k05", "k06"], b"v", 256);
    let (page_c, _) = page_of(&["k10"], b"v", 256);
    let page_li = vec![page_a, page_b, page_c];
    let redistributed_li = vec![false, true, false];
    let sep_li = page_separator_li(&page_li, &first_sep, &redistributed_li).unwrap();
    assert_eq!(sep_li[0], first_sep);
    assert_eq!(sep_li[1], b"k05".to_vec());
    assert_eq!(sep_li[2], find_shortest_separator(b"k06", b"k10").to_vec());
  }

  /// 用异构 `(key, value, overflow)` 列表构建单页，返回 (页字节, 实际条数)。
  fn page_from(items: &[(&str, &[u8], bool)], page_size: usize) -> (Vec<u8>, usize) {
    let mut builder = DataPageBuilder::new(page_size, 8).unwrap();
    let mut added = 0;
    for (key, value, overflow) in items {
      if !builder
        .add(EntrySpec {
          key: key.as_bytes(),
          value,
          ts: 1,
          expire_ts: 0,
          overflow: *overflow,
          compression: CompressionType::None,
          large_value: false,
        })
        .unwrap()
      {
        break;
      }
      added += 1;
    }
    (builder.finish().unwrap(), added)
  }

  /// 重分配按字节中点而非条目数中点切分：变长 entry 下后者会把前页切到 < 1/4。
  #[test]
  fn redistribution_splits_by_byte_midpoint() {
    let page_size = 2048;
    let small = vec![b's'; 10];
    let large = vec![b'l'; 400];
    let mut items: Vec<(String, Vec<u8>)> = (0..40)
      .map(|index| (format!("k{index:03}"), small.clone()))
      .collect();
    for index in 0..3 {
      items.push((format!("z{index:03}"), large.clone()));
    }
    let ref_li: Vec<(&str, &[u8], bool)> = items
      .iter()
      .map(|(key, value)| (key.as_str(), value.as_slice(), false))
      .collect();
    let (prev, prev_cnt) = page_from(&ref_li, page_size);
    assert_eq!(prev_cnt, items.len(), "前页须容纳全部 entry");
    let (cur, cur_cnt) = page_from(&[("z003", small.as_slice(), false)], page_size);
    assert_eq!(cur_cnt, 1);

    let prev_len = usize::from(content_len(&prev).unwrap());
    let cur_len = usize::from(content_len(&cur).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(cur_len < page_size / 4, "cur={cur_len}");

    let mut page_li = vec![prev, cur];
    let mut redistributed_li = Vec::new();
    rebalance_data_page_li(&mut page_li, page_size, 8, &mut [], &mut redistributed_li).unwrap();
    for page in &page_li {
      let len = usize::from(content_len(page).unwrap());
      assert!(
        len >= page_size / 4 && len <= page_size * 3 / 4,
        "len={len}"
      );
    }
  }

  /// 构造覆盖共享前缀、restart 对齐、负数时间戳增量、TTL、压缩标志与大 value 的条目。
  fn probe_entry_li(scale: usize) -> Vec<RawEntry> {
    (0..24)
      .map(|idx| {
        let value_len = if idx % 7 == 3 {
          40 * scale
        } else {
          8 + idx % 5
        };
        // ts 故意乱序，制造大 / 负的时间戳增量。
        let ts = if idx % 5 == 1 { 1 } else { 7 + idx as u64 };
        RawEntry {
          key: format!("key-{idx:03}").into_bytes(),
          value: vec![b'v'; value_len],
          ptr: false,
          ts,
          expire: if idx % 6 == 2 { 999 } else { 0 },
          comp: if idx % 4 == 0 {
            CompressionType::Dictionary
          } else {
            CompressionType::None
          },
          large: idx % 11 == 5,
        }
      })
      .collect()
  }

  /// `split_len_li` 估出的前后页长度必须与实际编码逐字节一致（含放不下的判定）。
  #[test]
  fn split_lens_matches_encoded_page_lengths() {
    for &page_size in &[256usize, 512, 1024] {
      for &ri in &[1u16, 3, 8, 16] {
        let entry_li = probe_entry_li(if page_size >= 1024 { 4 } else { 1 });
        let (prefix_li, suffix_li) = split_len_li(&entry_li, ri).unwrap();
        let n = entry_li.len();
        for k in 1..n {
          let ctx = format!("page_size={page_size} ri={ri} k={k}");
          match build_raw_page(&entry_li[..k], page_size, ri).unwrap() {
            Some(page) => assert_eq!(
              prefix_li[k],
              usize::from(content_len(&page).unwrap()),
              "前缀页长度不一致 {ctx}"
            ),
            None => assert!(prefix_li[k] > page_size, "前缀页应放不下 {ctx}"),
          }
          match build_raw_page(&entry_li[k..], page_size, ri).unwrap() {
            Some(page) => assert_eq!(
              suffix_li[k],
              usize::from(content_len(&page).unwrap()),
              "后缀页长度不一致 {ctx}"
            ),
            None => assert!(suffix_li[k] > page_size, "后缀页应放不下 {ctx}"),
          }
        }
      }
    }
  }

  /// 旧逐点试编码算法（参考实现）：用于验证 `pick_split` 选点完全一致。
  fn brute_pick(entry_li: &[RawEntry], target: usize, page_size: usize, ri: u16) -> Option<usize> {
    let n = entry_li.len();
    if n < 2 {
      return None;
    }
    let mut builder = DataPageBuilder::new(page_size, ri).unwrap();
    let mut k0 = 0;
    for entry in entry_li.iter().take(n - 1) {
      let added = builder
        .add(EntrySpec {
          key: &entry.key,
          value: &entry.value,
          ts: entry.ts,
          expire_ts: entry.expire,
          overflow: entry.ptr,
          compression: entry.comp,
          large_value: entry.large,
        })
        .unwrap();
      if !added {
        break;
      }
      k0 += 1;
      if builder.current_size_estimate() >= target {
        break;
      }
    }
    let k0 = k0.max(1);
    for delta in 0..n {
      for k in [k0 + delta, k0.saturating_sub(delta)] {
        if k == 0 || k >= n {
          continue;
        }
        if build_raw_page(&entry_li[..k], page_size, ri)
          .unwrap()
          .is_some()
          && build_raw_page(&entry_li[k..], page_size, ri)
            .unwrap()
            .is_some()
        {
          return Some(k);
        }
      }
      if k0 + delta >= n && k0 <= delta {
        break;
      }
    }
    None
  }

  /// 线性估算的选点结果必须与旧算法（逐点试编码）完全一致。
  #[test]
  fn pick_split_matches_bruteforce_probe() {
    for &page_size in &[256usize, 512] {
      for &ri in &[1u16, 4, 8] {
        let entry_li = probe_entry_li(1);
        let (prefix_li, _) = split_len_li(&entry_li, ri).unwrap();
        let target = prefix_li[entry_li.len() / 2];
        assert_eq!(
          pick_split(&entry_li, target, page_size, ri).unwrap(),
          brute_pick(&entry_li, target, page_size, ri),
          "page_size={page_size} ri={ri}"
        );
      }
    }
  }

  /// 含 overflow 占位指针的页照常参与重分配，slot 的 `page_idx` 跟随条目迁移。
  #[test]
  fn redistribution_migrates_overflow_slot() {
    let page_size = 1024;
    let small = vec![b's'; 10];
    let mut items: Vec<(String, Vec<u8>, bool)> = (0..30)
      .map(|index| (format!("k{index:03}"), small.clone(), false))
      .collect();
    items.push(("z000".to_string(), vec![0xAB; 300], true));
    let ref_li: Vec<(&str, &[u8], bool)> = items
      .iter()
      .map(|(key, value, overflow)| (key.as_str(), value.as_slice(), *overflow))
      .collect();
    let (prev, prev_cnt) = page_from(&ref_li, page_size);
    assert_eq!(prev_cnt, items.len(), "前页须容纳全部 entry");
    let (cur, _) = page_from(&[("z001", small.as_slice(), false)], page_size);

    let prev_len = usize::from(content_len(&prev).unwrap());
    assert!(prev_len > page_size * 3 / 4, "prev={prev_len}");
    assert!(usize::from(content_len(&cur).unwrap()) < page_size / 4);

    let mut slot_li = vec![OverflowSlot {
      page_idx: 0,
      key: b"z000".to_vec(),
      value: vec![0u8; 300],
    }];
    let mut page_li = vec![prev, cur];
    let mut redistributed_li = Vec::new();
    rebalance_data_page_li(
      &mut page_li,
      page_size,
      8,
      &mut slot_li,
      &mut redistributed_li,
    )
    .unwrap();
    // 占位条目前移到第二页，slot 必须跟随，且指针仍可按 key 定位回填。
    assert_eq!(slot_li[0].page_idx, 1);
    let range_li = locate_value_range_li(&page_li[1], &[b"z000"]).unwrap();
    assert_eq!(range_li[0].map(|(begin, end)| end - begin), Some(300));
  }

  /// 空表首次批量写入（`write_new_data_page`）同样做四分页重分配，消除稀疏尾页。
  #[test]
  fn fresh_write_rebalances_tail_page() {
    use fkv_io_mem::MemStore;
    use fkv_mapping::{AllocMode, PageMapper};
    use fkv_types::TableIdent;

    pollster::block_on(async {
      let page_size = 128;
      let restart_interval = 4;
      let layout = PageLayout::new(page_size, restart_interval, restart_interval, None, 16);
      // 6 条 8B value + 2 条 1B value 把首页填到接近满（仅余极少字节），
      // 第 9 条 1B value 只能单独成 31 字节（< 1/4 的 32）的稀疏尾页。
      let batch_li: Vec<BatchEntry> = (0..9)
        .map(|index| {
          let value = if index >= 6 {
            vec![b'v'; 1]
          } else {
            vec![b'v'; 8]
          };
          BatchEntry::upsert(format!("k{index:02}").into_bytes(), value, 1)
        })
        .collect();

      let MergeOutput { page_li: raw, .. } = merge_data_pages_with_options(
        &[],
        &batch_li,
        MergeOption {
          page_size,
          restart_interval,
          overflow_pointers: 16,
          dict: None,
          now_ms: 0,
          ttl_upsert_sink: None,
        },
      )
      .unwrap();
      let raw_len_li: Vec<usize> = raw
        .iter()
        .map(|page| usize::from(content_len(page).unwrap()))
        .collect();
      assert_eq!(raw_len_li.len(), 2, "{raw_len_li:?}");
      assert!(raw_len_li[0] > page_size * 3 / 4, "{raw_len_li:?}");
      assert!(raw_len_li[1] < page_size / 4, "{raw_len_li:?}");

      let store = MemStore::new(page_size);
      let tbl = TableIdent::new("t", 1).unwrap();
      let mut mapper = PageMapper::new(AllocMode::Append, 0);
      let entry_li: Vec<&BatchEntry> = batch_li.iter().collect();
      let mut pending = PendingWrite::default();
      let child_li =
        write_new_data_page(&store, &tbl, &mut mapper, &entry_li, layout, &mut pending)
          .await
          .unwrap();
      // 首写页在批末统一落盘（perf-02）：测试需先 flush 再读回。
      pending.flush(&store, &tbl).await.unwrap();
      assert_eq!(child_li.len(), raw_len_li.len());
      let mut buf = vec![0u8; page_size];
      for idx in 0..child_li.len() {
        let file_page_id = mapper.file_page_id(child_li.page_id(idx).unwrap()).unwrap();
        store.read_page(&tbl, file_page_id, &mut buf).await.unwrap();
        let len = usize::from(content_len(&buf).unwrap());
        assert!(len >= page_size / 4, "len={len}");
      }
    });
  }
}
