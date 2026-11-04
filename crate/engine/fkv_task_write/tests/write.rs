use fkv_btree::BatchEntry;
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_read::read_value;
use fkv_task_scan::{ScanArgs, scan_range};
use fkv_task_write::{WriteArgs, write_batch};
use fkv_types::{PageId, TableIdent};

#[test]
fn write_read_scan() {
  pollster::block_on(async {
    let store = MemStore::new(512);
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let batch = [
      BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1),
      BatchEntry::upsert(b"b".to_vec(), b"B".to_vec(), 1),
      BatchEntry::upsert(b"c".to_vec(), b"C".to_vec(), 1),
    ];
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 512,
        restart_interval: 16,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"b", 512, None)
        .await
        .unwrap(),
      Some(b"B".to_vec())
    );
    let all = scan_range(
      &store,
      ScanArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mapper,
        begin: b"",
        end: b"",
        page_size: 512,
        max_entries: 0,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 3);

    let batch = [
      BatchEntry::delete(b"b".to_vec(), 2),
      BatchEntry::upsert(b"d".to_vec(), b"D".to_vec(), 2),
    ];
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 512,
        restart_interval: 16,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"b", 512, None)
        .await
        .unwrap(),
      None
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"d", 512, None)
        .await
        .unwrap(),
      Some(b"D".to_vec())
    );
  });
}

#[test]
fn multi_page_cow() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 2).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut batch = Vec::new();
    for i in 0..100u32 {
      batch.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 30],
        1,
      ));
    }
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 128,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    let all = scan_range(
      &store,
      ScanArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mapper,
        begin: b"",
        end: b"",
        page_size: 128,
        max_entries: 0,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 100);

    let batch = [
      BatchEntry::upsert(b"k050".to_vec(), b"new".to_vec(), 2),
      BatchEntry::delete(b"k000".to_vec(), 2),
    ];
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 128,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k050", 128, None)
        .await
        .unwrap(),
      Some(b"new".to_vec())
    );
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k000", 128, None)
        .await
        .unwrap(),
      None
    );
    let all = scan_range(
      &store,
      ScanArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mapper,
        begin: b"",
        end: b"",
        page_size: 128,
        max_entries: 0,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 99);
  });
}

use core::cell::RefCell;

use fkv_error::Result as FkvResult;
use fkv_types::FilePageId;

struct CountingStore {
  inner: MemStore,
  write_count: RefCell<usize>,
}

impl CountingStore {
  fn new(page_size: usize) -> Self {
    Self {
      inner: MemStore::new(page_size),
      write_count: RefCell::new(0),
    }
  }

  fn take_write_count(&self) -> usize {
    let count = *self.write_count.borrow();
    *self.write_count.borrow_mut() = 0;
    count
  }
}

impl PageIo for CountingStore {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> FkvResult<()> {
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    src: &[u8],
  ) -> FkvResult<()> {
    *self.write_count.borrow_mut() += 1;
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> FkvResult<()> {
    self.inner.sync_data(tbl).await
  }
}

#[test]
fn cow_writes_only_affected_pages() {
  pollster::block_on(async {
    let store = CountingStore::new(128);
    let tbl = TableIdent::new("demo", 3).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut batch = Vec::new();
    for i in 0..100u32 {
      batch.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 30],
        1,
      ));
    }
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 128,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    let initial_writes = store.take_write_count();
    assert!(initial_writes > 10);

    let batch = [BatchEntry::upsert(b"k050".to_vec(), b"new".to_vec(), 2)];
    let _root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 128,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    let cow_writes = store.take_write_count();
    assert!(
      cow_writes < initial_writes / 2,
      "cow writes {cow_writes} should be far less than {initial_writes}"
    );
  });
}

#[test]
fn batch_same_page_one_cow() {
  pollster::block_on(async {
    let store = CountingStore::new(512);
    let tbl = TableIdent::new("demo", 4).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut batch = Vec::new();
    for i in 0..20u32 {
      batch.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 8],
        1,
      ));
    }
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 512,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    let _ = store.take_write_count();

    let batch: Vec<BatchEntry> = (0..5u32)
      .map(|i| BatchEntry::upsert(format!("k{i:03}").into_bytes(), vec![b'x'; 8], 2))
      .collect();
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 512,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    let writes = store.take_write_count();
    assert!(writes <= 8, "grouped batch wrote {writes} pages");
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k003", 512, None)
        .await
        .unwrap(),
      Some(vec![b'x'; 8])
    );
  });
}

#[test]
fn delete_many_rebalances_index() {
  pollster::block_on(async {
    let store = MemStore::new(128);
    let tbl = TableIdent::new("demo", 5).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let mut batch = Vec::new();
    for i in 0..100u32 {
      batch.push(BatchEntry::upsert(
        format!("k{i:03}").into_bytes(),
        vec![b'v'; 30],
        1,
      ));
    }
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: PageId::MAX,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 128,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    let batch: Vec<BatchEntry> = (0..90u32)
      .map(|i| BatchEntry::delete(format!("k{i:03}").into_bytes(), 2))
      .collect();
    let root = write_batch(
      &store,
      WriteArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mut mapper,
        batch: &batch,
        page_size: 128,
        restart_interval: 8,
        dict: None,
      },
    )
    .await
    .unwrap();
    let all = scan_range(
      &store,
      ScanArgs {
        tbl: &tbl,
        root_id: root,
        mapper: &mapper,
        begin: b"",
        end: b"",
        page_size: 128,
        max_entries: 0,
        dict: None,
      },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 10);
    assert_eq!(
      read_value(&store, &tbl, root, &mapper, b"k095", 128, None)
        .await
        .unwrap(),
      Some(vec![b'v'; 30])
    );
  });
}
