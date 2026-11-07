//! 数据面请求入口：默认 timeout / 队列超时包装与便捷方法。

use std::time::{Duration, Instant};

use fkv_api::{FloorReply, LargeValueReply, Request, Response, ScanPageOption, ScanReply};
use fkv_error::{KvError, Result};
use fkv_types::TableIdent;

use crate::Runtime;

/// 控制面请求不得走按分区路由的数据面入口（见 [`Request::is_control_plane`]）。
fn reject_control_plane(req: &Request) -> Result<()> {
  if req.is_control_plane() {
    return Err(KvError::InvalidArgs);
  }
  Ok(())
}

impl Runtime {
  /// 数据面请求：套用 [`Runtime::with_request_timeout`] /
  /// [`Runtime::with_queue_timeout`] 的默认值；未配置时与旧行为一致。
  ///
  /// 控制面请求（`AttachShard` / `DetachShard` / `Shutdown`）拒绝：它们按 shard
  /// 而非按 `TableIdent` 定位，走这里会绕过 attach 身份记录与视图收尾，并按
  /// 调用方的 `tbl` 清错 shard。请用 [`Runtime::attach_shard`] /
  /// [`Runtime::detach_shard`] / [`Runtime::shutdown`]。
  pub async fn request(&self, tbl: &TableIdent, req: Request) -> Result<Response> {
    reject_control_plane(&req)?;
    self.route(tbl).request(req).await
  }

  /// 显式 deadline 的请求，覆盖运行时默认值。
  pub async fn request_deadline(
    &self,
    tbl: &TableIdent,
    req: Request,
    timeout: Duration,
  ) -> Result<Response> {
    reject_control_plane(&req)?;
    self.route(tbl).request_deadline(req, timeout).await
  }

  /// 只设服务端截止时刻的请求（本端不兜底）；过期由 shard 回 `Timeout`。
  pub async fn request_at(
    &self,
    tbl: &TableIdent,
    req: Request,
    deadline: Instant,
  ) -> Result<Response> {
    reject_control_plane(&req)?;
    self.route(tbl).request_at(req, deadline).await
  }

  pub async fn put(&self, tbl: &TableIdent, batch_li: Vec<fkv_btree::BatchEntry>) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Put {
          tbl: tbl.clone(),
          batch_li,
          term: None,
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn put_with_term(
    &self,
    tbl: &TableIdent,
    batch_li: Vec<fkv_btree::BatchEntry>,
    term: u64,
  ) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Put {
          tbl: tbl.clone(),
          batch_li,
          term: Some(term),
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn put_large(
    &self,
    tbl: &TableIdent,
    batch_li: Vec<fkv_btree::BatchEntry>,
  ) -> Result<()> {
    match self
      .request(
        tbl,
        Request::PutLarge {
          tbl: tbl.clone(),
          batch_li,
          term: None,
        },
      )
      .await?
    {
      Response::Put => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn get_large(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<LargeValueReply>> {
    match self
      .request(
        tbl,
        Request::GetLarge {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::GetLarge(value) => Ok(value),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn read(&self, tbl: &TableIdent, key: &[u8]) -> Result<Vec<u8>> {
    self.get(tbl, key).await?.ok_or(KvError::NotFound)
  }

  pub async fn floor(&self, tbl: &TableIdent, key: &[u8]) -> Result<FloorReply> {
    match self
      .request(
        tbl,
        Request::Floor {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::Floor(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn get(&self, tbl: &TableIdent, key: &[u8]) -> Result<Option<Vec<u8>>> {
    match self
      .request(
        tbl,
        Request::Get {
          tbl: tbl.clone(),
          key: key.to_vec(),
        },
      )
      .await?
    {
      Response::Get(value) => Ok(value),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries,
          option: ScanPageOption::default(),
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply.entry_li),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan_page(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    option: ScanPageOption,
  ) -> Result<ScanReply> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries: 0,
          option,
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn scan_opt(
    &self,
    tbl: &TableIdent,
    begin: &[u8],
    end: &[u8],
    max_entries: usize,
    option: ScanPageOption,
  ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    match self
      .request(
        tbl,
        Request::Scan {
          tbl: tbl.clone(),
          begin: begin.to_vec(),
          end: end.to_vec(),
          max_entries,
          option,
        },
      )
      .await?
    {
      Response::Scan(reply) => Ok(reply.entry_li),
      _ => Err(KvError::Corrupted),
    }
  }
}
