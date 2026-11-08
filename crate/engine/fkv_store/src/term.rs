//! term 文件读写与带 term 的写入入口。

use fkv_btree::BatchEntry;
use fkv_error::{KvError, Result};
use fkv_io::{ManifestIo, PageIo, TermIo};
use fkv_types::TableIdent;

use crate::Store;

impl<I: PageIo + ManifestIo + TermIo> Store<I> {
  pub async fn claim_term(&self, branch: &str, shard_id: u32, term: u64) -> Result<()> {
    self.io.upsert_term(branch, shard_id, term).await?;
    Ok(())
  }

  pub async fn read_term(&self, branch: &str, shard_id: u32) -> Result<Option<u64>> {
    self.io.read_term(branch, shard_id).await
  }

  pub async fn put_with_term(
    &mut self,
    tbl: &TableIdent,
    batch_li: &[BatchEntry],
    branch: &str,
    shard_id: u32,
    term: u64,
  ) -> Result<()> {
    if self
      .io
      .read_term(branch, shard_id)
      .await?
      .is_some_and(|current| current > term)
    {
      return Err(KvError::ExpiredTerm);
    }
    self.put(tbl, batch_li).await
  }
}
