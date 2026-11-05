//! r10 #3：`prewarm_segment_file` 的 epoch 必须在 `remote.get` **之前**取。
//!
//! 场景：prewarm 挂在 `remote.get` 期间，该表被 `evict_table` 驱逐。修复后
//! `install_segment_file` 复核 epoch 不一致 → `TryAgain`，被驱逐的表不会被
//! 重新登记（否则 reopen 会把本地残留当权威，已 detach 的表「复活」）。
//!
//! 用例手工 poll（不用 sleep、不用额外 executor）：先 poll 到 `Pending`（真的
//! 停在远程取），期间驱逐，再放行远程取。
//!
//! **回退即失败（已实测，2026-11-05T02:14Z）**：把 `fkv_cloud` 里
//! `let epoch = self.evict_epoch.get();` 挪到 `remote.get` 之后，本用例
//! 立即失败 —— `panicked at segment_prewarm_epoch.rs:154: 驱逐后 install 必须回
//! TryAgain，实际 Ready(Ok(true))`（第二次断言 `resident_bytes() == 0` 也同样会挂）。
//! 复原后重跑 104 条云用例全绿。
//!
//! 说明：该「回退验证」需要在 `fkv_cloud/src` 上做一次临时改动，只应在确认
//! 该文件无并发写手时执行（本次实测窗口内确有他方在编辑该文件，属侥幸未冲突）。

use std::{
  cell::Cell,
  pin::pin,
  rc::Rc,
  task::{Context, Poll},
};

use bytes::Bytes;
use fkv_cloud::CloudStore;
use fkv_error::{KvError, Result};
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_object::{MemObjectStore, ObjectMeta, ObjectStore};
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};
use futures::{future::poll_fn, task::noop_waker};

const SEGMENT_SIZE: usize = 4096;
/// 每文件 4 槽（`segments_per_file_shift = 2`）。
const SEGMENT_SHIFT: u8 = 2;

/// 手动放行闸门：`entered` 记录「确实进过远程取」。
#[derive(Default)]
struct Gate {
  entered: Cell<bool>,
  released: Cell<bool>,
}

/// 只在 `get` 处挂起的对象存储；其余方法直接转发 `MemObjectStore`。
struct GatedGetStore {
  inner: MemObjectStore,
  gate: Rc<Gate>,
}

impl GatedGetStore {
  fn new(gate: Rc<Gate>) -> Self {
    Self {
      inner: MemObjectStore::new(),
      gate,
    }
  }

  /// 未放行时挂起；手工 poll 下每次 poll 重新判定，无需 waker 唤醒。
  async fn block_at_get(&self) {
    if !self.gate.released.get() {
      self.gate.entered.set(true);
    }
    poll_fn(|_| {
      if self.gate.released.get() {
        Poll::Ready(())
      } else {
        Poll::Pending
      }
    })
    .await
  }
}

impl ObjectStore for GatedGetStore {
  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    self.block_at_get().await;
    self.inner.get(key).await
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    self.inner.put(key, value, if_match, if_none_match).await
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    self.inner.head(key).await
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.inner.delete(key).await
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    self.inner.list(prefix).await
  }
}

/// `<prefix>/<table>.<partition>/<filename>`（对齐 C++ `ComposeKey`）。
fn segment_key(tbl: &TableIdent, file_id: u64) -> String {
  format!(
    "fkv/{tbl}/{}",
    fkv_types::segment_file_name(file_id, fkv_types::MAIN_BRANCH_NAME, 0)
  )
}

#[test]
fn prewarm_epoch_taken_before_remote_get_prevents_revive() {
  pollster::block_on(async {
    let tbl = TableIdent::new("demo", 1).unwrap();
    let gate = Rc::new(Gate::default());
    let remote = GatedGetStore::new(gate.clone());
    // 远端预置一个完整段文件：槽数 = 1 << shift，长度必须精确等于槽数 × 段大小。
    let slot_num = 1usize << SEGMENT_SHIFT;
    remote
      .inner
      .put(
        &segment_key(&tbl, 0),
        Bytes::from(vec![7u8; slot_num * SEGMENT_SIZE]),
        None,
        false,
      )
      .await
      .unwrap();

    let cloud = CloudStore::new(
      MemStore::new(512),
      remote,
      "fkv",
      DEFAULT_PAGES_PER_FILE_SHIFT,
    )
    .with_segment_config(SEGMENT_SIZE, SEGMENT_SHIFT);

    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut prewarm = pin!(cloud.prewarm_segment_file(&tbl, 0));
    assert!(
      prewarm.as_mut().poll(&mut cx).is_pending(),
      "本地缺该段文件，应停在 remote.get 上"
    );
    assert!(
      gate.entered.get(),
      "必须真的走到远程取（否则本用例没有卡在 get 之后的空间）"
    );

    // get 期间驱逐该表：epoch 推进，且摘掉在飞登记。
    cloud.evict_table(&tbl);

    // 放行远程取：install 必须放弃，而不是把已驱逐的表重新登记。
    gate.released.set(true);
    match prewarm.as_mut().poll(&mut cx) {
      Poll::Ready(Err(KvError::TryAgain)) => {}
      other => panic!("驱逐后 install 必须回 TryAgain，实际 {other:?}"),
    }
    assert_eq!(
      cloud.resident_bytes(),
      0,
      "已驱逐的表不得因在飞 install 完成而重新登记"
    );
  });
}
