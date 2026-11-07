//! perf-10：点读下降的单页读改走 owned 直读后，IndexCache 命中的点读不再
//! 租借读池（只剩 data 页一次文件读，缓冲由调用方 `PageBuf` 直接提供）。

use std::cell::RefCell;

use fkv_btree::IndexCache;
use fkv_io::PageIo;
use fkv_io_compio::CompioStore;
use fkv_mapping::{AllocMode, MappingSnapshot, PageMapper, SnapshotRegistry, SnapshotSource};
use fkv_page::{CompressionType, DataPageBuilder, EntrySpec, IndexPageBuilder};
use fkv_task_read::{IndexRead, ReadCtx, read_entry};
use fkv_testkit::TempRoot;
use fkv_types::{NULL_PAGE, PageId, TableIdent};

const PAGE_SIZE: usize = 512;

/// 测试用快照：登记表只存弱引用，临时 registry 丢弃后快照仍有效。
fn snap(tbl: &TableIdent, mapper: &PageMapper, root_id: PageId) -> MappingSnapshot {
  SnapshotRegistry::new().open(
    tbl,
    SnapshotSource {
      data: mapper,
      segment: None,
      root_id,
      ttl_root_id: NULL_PAGE,
    },
  )
}

fn data_page() -> Vec<u8> {
  let mut builder = DataPageBuilder::new(PAGE_SIZE, 16).unwrap();
  for (key, value, ts) in [(&b"a"[..], &b"A"[..], 1), (&b"b"[..], &b"B"[..], 2)] {
    builder
      .add(EntrySpec {
        key,
        value,
        ts,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
  }
  builder.finish().unwrap()
}

fn index_page(leaf: bool, leftmost: PageId) -> Vec<u8> {
  let mut builder = IndexPageBuilder::new(PAGE_SIZE, 16, leaf).unwrap();
  builder.add(b"", leftmost).unwrap();
  builder.finish().unwrap()
}

/// 两层 index + data 页：首读回填缓存（3 次单页读）；再读两级 index 命中，
/// 只读 data 页一次且不租借读池。
#[compio::test]
async fn cached_point_read_takes_no_read_pool_buffer() {
  let root = TempRoot::new("perf10_read_read_pool");
  let tbl = TableIdent::new("demo", 1).unwrap();
  let store = CompioStore::new(root.path().to_path_buf(), PAGE_SIZE, 2);
  let mut mapper = PageMapper::new(AllocMode::Append, 0);
  let data_id = mapper.allocate_page();
  let leaf_index_id = mapper.allocate_page();
  let root_id = mapper.allocate_page();
  mapper.set_file_page(data_id, 0);
  mapper.set_file_page(leaf_index_id, 1);
  mapper.set_file_page(root_id, 2);
  store.write_page(&tbl, 0, &data_page()).await.unwrap();
  store
    .write_page(&tbl, 1, &index_page(true, data_id))
    .await
    .unwrap();
  store
    .write_page(&tbl, 2, &index_page(false, leaf_index_id))
    .await
    .unwrap();

  let snapshot = snap(&tbl, &mapper, root_id);
  let cache = RefCell::new(IndexCache::with_capacity_bytes(0));
  let ctx = ReadCtx {
    index: IndexRead::new(Some(&cache), None),
    ..ReadCtx::new(&snapshot, IndexRead::none())
  };

  // 首读：root / leaf index 未命中各读一页并回填，随后读 data 页。
  let before_take = store.read_buf_take_num();
  let first = read_entry(&store, &tbl, b"a", PAGE_SIZE, ctx)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(first.value, b"A");

  // 再读：IndexCache 两级命中，只剩 data 页一次文件读。
  let before_read = store.read_at_num();
  let entry = read_entry(&store, &tbl, b"b", PAGE_SIZE, ctx)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(entry.value, b"B");
  assert_eq!(store.read_at_num() - before_read, 1);
  assert_eq!(
    store.read_buf_take_num(),
    before_take,
    "点读下降（含 IndexCache 命中）不允许租借读池缓冲"
  );
}
