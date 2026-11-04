use std::{
  cell::Cell,
  thread::{sleep, spawn},
  time::Duration,
};

use fkv_error::{KvError, Result};
use fkv_io::ManifestIo;
use fkv_manifest::{BranchManifestMetadata, ManifestBuilder, SnapshotArgs};
use fkv_task_reopen::{
  AutoReopenOptions, Sleeper, auto_reopen_with_size_with_sleeper, auto_reopen_with_sleeper,
};
use fkv_types::{PageId, TableIdent};

fn valid_manifest() -> Vec<u8> {
  let branch = BranchManifestMetadata::new("main", 0);
  ManifestBuilder::new()
    .snapshot(SnapshotArgs {
      root_id: PageId::MAX,
      ttl_root: PageId::MAX,
      max_fp_id: 0,
      dict_bytes: b"",
      mapping_value_li: &[],
      branch_metadata: &branch,
      segment_value_li: None,
      max_segment_fp_id: 0,
    })
    .unwrap()
}

struct FlakyManifest {
  fail_until: usize,
  read_num: Cell<usize>,
  bytes: Vec<u8>,
  forced_error: Option<KvError>,
}

impl FlakyManifest {
  fn new(fail_until: usize, bytes: Vec<u8>) -> Self {
    Self {
      fail_until,
      read_num: Cell::new(0),
      bytes,
      forced_error: None,
    }
  }
}

impl ManifestIo for FlakyManifest {
  async fn append_manifest(&self, _tbl: &TableIdent, _record: &[u8]) -> Result<()> {
    Err(KvError::InvalidArgs)
  }

  async fn switch_manifest(&self, _tbl: &TableIdent, _record: &[u8]) -> Result<()> {
    Err(KvError::InvalidArgs)
  }

  async fn read_manifest(&self, _tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    if let Some(err) = self.forced_error {
      return Err(err);
    }
    let read_num = self.read_num.get() + 1;
    self.read_num.set(read_num);
    if read_num > self.fail_until {
      Ok(Some(self.bytes.clone()))
    } else {
      Ok(None)
    }
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
    Err(KvError::InvalidArgs)
  }

  async fn list_manifest_file_name_li(&self, _tbl: &TableIdent) -> Result<Vec<String>> {
    Ok(Vec::new())
  }

  async fn read_manifest_file(&self, _tbl: &TableIdent, _name: &str) -> Result<Option<Vec<u8>>> {
    Ok(None)
  }

  async fn remove_manifest_file(&self, _tbl: &TableIdent, _name: &str) -> Result<()> {
    Ok(())
  }
}

#[derive(Default)]
struct CountingSleeper {
  call_num: Cell<usize>,
  total: Cell<Duration>,
}

impl Sleeper for CountingSleeper {
  async fn sleep(&self, duration: Duration) {
    self.call_num.set(self.call_num.get() + 1);
    self.total.set(self.total.get() + duration);
  }
}

#[test]
fn retries_until_manifest_appears() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let io = FlakyManifest::new(2, valid_manifest());
    let sleeper = CountingSleeper::default();
    let (state, size) = auto_reopen_with_size_with_sleeper(
      &io,
      &tbl,
      AutoReopenOptions::new(3, Duration::from_millis(5)),
      &sleeper,
    )
    .await
    .unwrap();
    assert_eq!(state.root_id, PageId::MAX);
    assert!(size > 0);
    assert_eq!(io.read_num.get(), 3);
    assert_eq!(sleeper.call_num.get(), 2);
    assert_eq!(sleeper.total.get(), Duration::from_millis(10));
  });
}

#[test]
fn exhausts_retries_and_returns_missing() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let io = FlakyManifest::new(10, valid_manifest());
    let sleeper = CountingSleeper::default();
    let state = auto_reopen_with_sleeper(
      &io,
      &tbl,
      AutoReopenOptions::new(1, Duration::from_millis(2)),
      &sleeper,
    )
    .await;
    assert_eq!(state.unwrap_err(), KvError::ResourceMissing);
    assert_eq!(io.read_num.get(), 2);
    assert_eq!(sleeper.call_num.get(), 1);
  });
}

#[test]
fn non_retryable_error_is_not_retried() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let mut io = FlakyManifest::new(0, valid_manifest());
    io.forced_error = Some(KvError::Corrupted);
    let sleeper = CountingSleeper::default();
    let state = auto_reopen_with_sleeper(
      &io,
      &tbl,
      AutoReopenOptions::new(5, Duration::from_millis(2)),
      &sleeper,
    )
    .await;
    assert_eq!(state.unwrap_err(), KvError::Corrupted);
    assert_eq!(io.read_num.get(), 0);
    assert_eq!(sleeper.call_num.get(), 0);
  });
}

#[test]
fn auto_reopen_waits_for_cloud_manifest() {
  use fkv_cloud::CloudStore;
  use fkv_io_mem::MemStore;
  use fkv_object::{MemObjectStore, ObjectStore};
  use fkv_task_reopen::auto_reopen;

  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let remote = MemObjectStore::new();
    let cloud = CloudStore::new(MemStore::new(512), remote.clone(), "fkv", 18);
    let manifest = valid_manifest();
    let writer = spawn(move || {
      sleep(Duration::from_millis(30));
      pollster::block_on(remote.put("fkv/demo.1/manifest", &manifest, None, false)).unwrap();
    });

    let state = auto_reopen(
      &cloud,
      &tbl,
      AutoReopenOptions::new(50, Duration::from_millis(10)),
    )
    .await
    .unwrap();
    writer.join().unwrap();
    assert_eq!(state.root_id, PageId::MAX);
    assert!(cloud.local().read_manifest(&tbl).await.unwrap().is_some());
  });
}
