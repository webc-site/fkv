//! `TermIo`：branch / shard term 读写，立即落 durable。

use fkv_error::{KvError, Result};
use fkv_io::TermIo;

use crate::{SimStore, op::Op, state::log_op};

impl TermIo for SimStore {
  /// term 立即落 durable（对齐 C++ `CURRENT_TERM` 写后 fsync），不参与 volatile 覆盖。
  async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadTerm {
        branch: branch.to_owned(),
        shard_id,
      },
    )?;
    Ok(
      state
        .durable
        .term_map
        .get(&(branch.to_owned(), shard_id))
        .copied(),
    )
  }

  async fn upsert_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<u64> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::UpsertTerm {
        branch: branch.to_owned(),
        shard_id,
        term,
      },
    )?;
    let key = (branch.to_owned(), shard_id);
    if let Some(current) = state.durable.term_map.get(&key).copied()
      && current > term
    {
      return Err(KvError::ExpiredTerm);
    }
    state.durable.term_map.insert(key, term);
    Ok(term)
  }

  async fn remove_term(&self, branch: &str, shard_id: u32) -> Result<()> {
    // Sim 的 term 本来就立即 durable；直接删等价于 C++ unlink + 目录 fsync。
    self
      .lock()
      .durable
      .term_map
      .remove(&(branch.to_owned(), shard_id));
    Ok(())
  }
}
