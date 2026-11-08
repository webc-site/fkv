use std::{cell::Cell, future::Future};

use fkv_error::KvError;
use fkv_io::{FileIo, PageIo, SegmentIo};
use fkv_io_mem::MemStore;
use fkv_mapping::{AllocMode, PageMapper};
use fkv_task_compact::{compact_file_li, compact_segment_file_li};
use fkv_task_gc::{
  InflightGuard, add_mapping_retained, gc_files_with_retained, gc_segment_files_with_retained,
};
use fkv_types::{FilePageId, TableIdent};
use gxhash::HashSet;

/// 读走内存后端，写一律失败：验证 compact 入口自建批次在 IO 失败时回滚。
struct WriteFailIo(MemStore);

impl PageIo for WriteFailIo {
  fn read_page<'a>(
    &'a self,
    tbl: &'a TableIdent,
    file_page_id: fkv_types::FilePageId,
    dst: &'a mut [u8],
  ) -> impl Future<Output = fkv_error::Result<()>> + 'a {
    self.0.read_page(tbl, file_page_id, dst)
  }

  async fn write_page(
    &self,
    _tbl: &TableIdent,
    _file_page_id: fkv_types::FilePageId,
    _src: &[u8],
  ) -> fkv_error::Result<()> {
    Err(KvError::Corrupted)
  }

  async fn sync_data(&self, _tbl: &TableIdent) -> fkv_error::Result<()> {
    Ok(())
  }
}

#[test]
fn compact_then_gc() {
  pollster::block_on(async {
    let store = MemStore::with_pages_per_file(8, 1);
    let tbl = TableIdent::new("demo", 1).unwrap();
    store.write_page(&tbl, 0, b"12345678").await.unwrap();
    store.write_page(&tbl, 2, b"abcdefgh").await.unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 4);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    mapper.set_file_page(p1, 2);

    assert_eq!(
      compact_file_li(&store, &tbl, &mut mapper, &[0], 8, 1)
        .await
        .unwrap(),
      1
    );
    assert_eq!(mapper.file_page_id(p0).unwrap(), 6);
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 6, &mut dst).await.unwrap();
    assert_eq!(&dst, b"12345678");

    let mut retained = HashSet::default();
    add_mapping_retained(mapper.tbl(), &mut retained);
    assert_eq!(
      gc_files_with_retained(&store, &tbl, &retained, &InflightGuard::default(), 1)
        .await
        .unwrap(),
      1
    );
    let mut file_li = store.list_file_id_li(&tbl).await.unwrap();
    file_li.sort_unstable();
    assert_eq!(file_li, vec![1, 3]);
  });
}

/// sto-56b：segment 文件按 live 段搬迁，搬空后 GC 删除旧文件，数据保持可读。
#[test]
fn compact_segment_then_gc() {
  pollster::block_on(async {
    let store = MemStore::new(64).with_segment_config(64, 1);
    let tbl = TableIdent::new("demo", 2).unwrap();
    let seg_a = [b'a'; 64];
    let seg_b = [b'b'; 64];
    store.write_segment(&tbl, 0, &seg_a).await.unwrap();
    store.write_segment(&tbl, 1, &seg_b).await.unwrap();

    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let p0 = mapper.allocate_page();
    let p1 = mapper.allocate_page();
    let p2 = mapper.allocate_page();
    // p2 释放后 file 1 只剩死槽；file 0 仍有 2 个 live 段。
    mapper.free_page(p2).unwrap();

    assert_eq!(
      compact_segment_file_li(&store, &tbl, &mut mapper, &[0], 64, 1)
        .await
        .unwrap(),
      2
    );
    for (page_id, expect) in [(p0, &seg_a), (p1, &seg_b)] {
      let mut dst = [0u8; 64];
      store
        .read_segment(&tbl, mapper.file_page_id(page_id).unwrap(), &mut dst)
        .await
        .unwrap();
      assert_eq!(&dst, expect);
    }

    let mut retained = HashSet::default();
    add_mapping_retained(mapper.tbl(), &mut retained);
    assert_eq!(
      gc_segment_files_with_retained(&store, &tbl, &retained, &InflightGuard::default(), 1)
        .await
        .unwrap(),
      1
    );
    let mut file_li = store.list_segment_file_id_li(&tbl).await.unwrap();
    file_li.sort_unstable();
    assert!(!file_li.contains(&0), "空 segment 文件必须被回收");
  });
}

/// 直接调用 compact_file_li 中途写失败：入口批次必须回滚已完成的 remap。
#[test]
fn compact_file_rolls_back_on_write_failure() {
  pollster::block_on(async {
    let store = WriteFailIo(MemStore::with_pages_per_file(8, 1));
    let tbl = TableIdent::new("demo", 3).unwrap();
    store.0.write_page(&tbl, 0, b"12345678").await.unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 4);
    let p0 = mapper.allocate_page();
    mapper.set_file_page(p0, 0);
    let changed_len = mapper.changed_page_li().len();

    assert!(
      compact_file_li(&store, &tbl, &mut mapper, &[0], 8, 1)
        .await
        .is_err()
    );
    assert_eq!(mapper.file_page_id(p0).unwrap(), 0, "remap 必须回滚");
    assert_eq!(
      mapper.changed_page_li().len(),
      changed_len,
      "失败批次不得遗留 changed_page_li"
    );
  });
}

/// sem-60c：文件内搬移序按旧 fp 升序（C++ `FilePageLess` 堆排模型）。
///
/// 可达性：删除把逻辑页压回空闲链表头（LIFO），后续 `allocate_page` 复用该
/// page id 却从 append 分配器拿到更高的 fp——同文件内「逻辑页序 ≠ fp 序」
/// 由真实 mapper API 构造（allocate → free → allocate），非手工摆盘。
/// 此时批内搬移序决定新 fp 发号：C++ 按 fp 升序（background_write.cpp:66-98），
/// 旧 fkv 按逻辑页升序，两侧发号互换。锁定 C++ 模型的手推期望值。
#[test]
fn compact_moves_pages_in_old_fp_order_within_file() {
  pollster::block_on(async {
    // 每文件 8 页（shift=3）：fp 0..8 同属 file 0。
    let store = MemStore::with_pages_per_file(8, 3);
    let tbl = TableIdent::new("sem60c_data", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);

    let p0 = mapper.allocate_page(); // page 0 → fp 0
    let p1 = mapper.allocate_page(); // page 1 → fp 1
    assert_eq!(mapper.file_page_id(p0).unwrap(), 0);
    assert_eq!(mapper.file_page_id(p1).unwrap(), 1);
    mapper.free_page(p0).unwrap(); // page 0 回空闲链表头
    let p0_reused = mapper.allocate_page(); // 复用 page 0 → fp 2
    assert_eq!(p0_reused, p0, "空闲链表头必须被复用（可达性前提）");
    assert_eq!(mapper.file_page_id(p0).unwrap(), 2);
    // 盘面：file 0 的 live 页 page 序 [p0, p1] 与 fp 序 [fp1(p1), fp2(p0)] 相反。
    store.write_page(&tbl, 1, b"p1......").await.unwrap();
    store.write_page(&tbl, 2, b"p0......").await.unwrap();

    assert_eq!(
      compact_file_li(&store, &tbl, &mut mapper, &[0], 8, 3)
        .await
        .unwrap(),
      2
    );
    // C++ fp 升序模型：fp1(p1) 先搬拿新 fp 3，fp2(p0) 后搬拿新 fp 4。
    // 旧口径（逻辑页升序）会给 p0 发 3、p1 发 4 —— 互换即分叉。
    assert_eq!(mapper.file_page_id(p1).unwrap(), 3);
    assert_eq!(mapper.file_page_id(p0).unwrap(), 4);
    let mut dst = [0u8; 8];
    store.read_page(&tbl, 3, &mut dst).await.unwrap();
    assert_eq!(&dst, b"p1......", "内容必须跟随 fp 升序的搬移序");
    store.read_page(&tbl, 4, &mut dst).await.unwrap();
    assert_eq!(&dst, b"p0......");
  });
}

/// sem-60c segment 侧：段文件内搬移序同构按旧 fp 升序
/// （C++ `DoCompactSegmentFile`，background_write.cpp:393）。
#[test]
fn compact_moves_segments_in_old_fp_order_within_file() {
  pollster::block_on(async {
    let store = MemStore::new(64).with_segment_config(64, 3);
    let tbl = TableIdent::new("sem60c_seg", 1).unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);

    let s0 = mapper.allocate_page(); // 段 0 → fp 0
    let s1 = mapper.allocate_page(); // 段 1 → fp 1
    mapper.free_page(s0).unwrap();
    let s0_reused = mapper.allocate_page(); // 复用段 0 → fp 2
    assert_eq!(s0_reused, s0);
    let seg_a = [b'a'; 64];
    let seg_b = [b'b'; 64];
    store.write_segment(&tbl, 1, &seg_a).await.unwrap();
    store.write_segment(&tbl, 2, &seg_b).await.unwrap();

    assert_eq!(
      compact_segment_file_li(&store, &tbl, &mut mapper, &[0], 64, 3)
        .await
        .unwrap(),
      2
    );
    assert_eq!(mapper.file_page_id(s1).unwrap(), 3, "fp1(s1) 先搬");
    assert_eq!(mapper.file_page_id(s0).unwrap(), 4, "fp2(s0) 后搬");
    let mut dst = [0u8; 64];
    store
      .read_segment(&tbl, mapper.file_page_id(s1).unwrap(), &mut dst)
      .await
      .unwrap();
    assert_eq!(&dst, &seg_a);
    store
      .read_segment(&tbl, mapper.file_page_id(s0).unwrap(), &mut dst)
      .await
      .unwrap();
    assert_eq!(&dst, &seg_b);
  });
}

/// 前 `fail_times` 次读返回 `TryAgain`（云后端「同组另一实例在写该段，让路」
/// 的等价错误），之后转发给内存后端。
struct FlakyReadIo {
  inner: MemStore,
  fail_times: u32,
  read_num: Cell<u32>,
}

impl FlakyReadIo {
  fn new(inner: MemStore, fail_times: u32) -> Self {
    Self {
      inner,
      fail_times,
      read_num: Cell::new(0),
    }
  }

  fn check(&self) -> fkv_error::Result<()> {
    let read_num = self.read_num.get();
    self.read_num.set(read_num + 1);
    if read_num < self.fail_times {
      return Err(KvError::TryAgain);
    }
    Ok(())
  }
}

impl PageIo for FlakyReadIo {
  async fn read_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.check()?;
    self.inner.read_page(tbl, file_page_id, dst).await
  }

  async fn write_page(
    &self,
    tbl: &TableIdent,
    file_page_id: FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_page(tbl, file_page_id, src).await
  }

  async fn sync_data(&self, tbl: &TableIdent) -> fkv_error::Result<()> {
    self.inner.sync_data(tbl).await
  }
}

impl SegmentIo for FlakyReadIo {
  async fn read_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    dst: &mut [u8],
  ) -> fkv_error::Result<()> {
    self.check()?;
    self.inner.read_segment(tbl, segment_fp_id, dst).await
  }

  async fn write_segment(
    &self,
    tbl: &TableIdent,
    segment_fp_id: FilePageId,
    src: &[u8],
  ) -> fkv_error::Result<()> {
    self.inner.write_segment(tbl, segment_fp_id, src).await
  }
}

/// 回退即失败：不重试时首个 `TryAgain` 就让整批 abort，冷读压缩系统性失败。
#[test]
fn compact_retries_retryable_page_read() {
  pollster::block_on(async {
    let tbl = TableIdent::new("retry_page", 1).unwrap();
    let store = FlakyReadIo::new(MemStore::with_pages_per_file(8, 1), 2);
    store.inner.write_page(&tbl, 0, b"12345678").await.unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 4);
    let p0 = mapper.allocate_page();
    mapper.set_file_page(p0, 0);

    assert_eq!(
      compact_file_li(&store, &tbl, &mut mapper, &[0], 8, 1)
        .await
        .unwrap(),
      1
    );
    assert_eq!(store.read_num.get(), 3, "两次 TryAgain 后必须继续搬页");
    let mut dst = [0u8; 8];
    store
      .read_page(&tbl, mapper.file_page_id(p0).unwrap(), &mut dst)
      .await
      .unwrap();
    assert_eq!(&dst, b"12345678");
  });
}

#[test]
fn compact_retries_retryable_segment_read() {
  pollster::block_on(async {
    let tbl = TableIdent::new("retry_segment", 1).unwrap();
    let store = FlakyReadIo::new(MemStore::new(64).with_segment_config(64, 1), 2);
    store
      .inner
      .write_segment(&tbl, 0, &[b'a'; 64])
      .await
      .unwrap();
    store
      .inner
      .write_segment(&tbl, 1, &[b'b'; 64])
      .await
      .unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 0);
    let _p0 = mapper.allocate_page();
    let _p1 = mapper.allocate_page();

    assert_eq!(
      compact_segment_file_li(&store, &tbl, &mut mapper, &[0], 64, 1)
        .await
        .unwrap(),
      2
    );
    assert_eq!(store.read_num.get(), 4, "2 段 + 2 次瞬时错误");
  });
}

/// 重试必须有界：持续返回 `TryAgain` 时按次数上限上抛，不死循环。
#[test]
fn compact_gives_up_after_retry_budget() {
  pollster::block_on(async {
    let tbl = TableIdent::new("retry_exhaust", 1).unwrap();
    let store = FlakyReadIo::new(MemStore::with_pages_per_file(8, 1), u32::MAX);
    store.inner.write_page(&tbl, 0, b"12345678").await.unwrap();
    let mut mapper = PageMapper::new(AllocMode::Append, 4);
    let p0 = mapper.allocate_page();
    mapper.set_file_page(p0, 0);

    assert_eq!(
      compact_file_li(&store, &tbl, &mut mapper, &[0], 8, 1).await,
      Err(KvError::TryAgain)
    );
    assert_eq!(store.read_num.get(), 8, "尝试次数必须有界");
  });
}
