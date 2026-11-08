//! 单分区控制面 API：reopen / GC / compact / truncate / archive / branch。

use fkv_api::{
  ArchiveReply, ArchiveTagInfo, BranchReply, CleanExpiredReply, ListPage, Request, Response,
};
use fkv_error::{KvError, Result};
use fkv_types::{FileId, InstallOutcome, TableIdent};

use crate::Runtime;

impl Runtime {
  /// 远端状态是权威的部署形态：Cloud（`cloud_enum` 只在 `Backend::Cloud` 装配）
  /// 与 StandbyReplica（`standby_replica` 由启动期 standby 服务装配决定）。
  fn remote_state_authoritative(&self) -> bool {
    self.cloud_enum.is_some() || self.standby_replica
  }

  /// 显式重新打开分区。Cloud + `prewarm_cloud_cache` 下成功后按表投递后台预热，
  /// 不阻塞本回执（对齐 C++ `ReopenTask::Reopen` → `PrewarmService::Prewarm`）。
  ///
  /// Cloud / StandbyReplica 下对齐 C++「缺失快照即成功清理」：远端没有该分区时
  /// `InstallExternalSnapshot` 清本地 manifest / 内存 RootMeta 后回 `NoError`
  /// （`page_manager.cpp` 的 `cleared_local_state` 分支），本层同样把 `NotFound`
  /// / `ResourceMissing` 收敛为 `Ok` —— 本地状态已由 `Store::reopen`
  /// （`reopen_with` 的 `evict_table`）或 standby 同步路径（`dispatch_reopen`
  /// 的 `clear_missing_table`：删本地 manifest + 清内存 `Store` / 打开登记）
  /// 清掉，不会以空表起跑，也不会继续读已删除分区的旧内存值。
  /// Local / Mem 保持 `ResourceMissing`：C++ 在 Local 下直接拒绝 `Reopen`
  /// （`InvalidArgs`），没有需要对齐的成功路径；`Store::reopen` 的显式失败语义
  /// 也不变（嵌入式 API 仍能看见缺失）。
  pub async fn reopen(&self, tbl: &TableIdent) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Reopen {
          tbl: tbl.clone(),
          // 显式 reopen 不置 clean：缺失收尾由本层按 Cloud / StandbyReplica
          // 模式对齐 C++ 的 NoError（见方法文档），不改变 shard 的延迟重试语义。
          clean: false,
        },
      )
      .await
    {
      Ok(Response::Reopen) => {
        self.signal_table_prewarm(tbl);
        Ok(())
      }
      Err(KvError::NotFound | KvError::ResourceMissing) if self.remote_state_authoritative() => {
        Ok(())
      }
      Err(err) => Err(err),
      Ok(_) => Err(KvError::Corrupted),
    }
  }

  /// 采纳外部（远端）当前快照：Cloud 强制回源 manifest，StandbyReplica 先追赶
  /// master，再由 shard 的 `install_external_snapshot` 安装。仅 Cloud /
  /// StandbyReplica 可用，其余模式回 `InvalidArgs`。
  ///
  /// 真正安装（`Installed`）后与 [`Self::reopen`] 一样投递后台 per-table 预热；
  /// `WaitingRemote` 没有可预热的远端分区，不投递。
  pub async fn reopen_external(&self, tbl: &TableIdent) -> Result<InstallOutcome> {
    match self
      .request(tbl, Request::ReopenExternal { tbl: tbl.clone() })
      .await?
    {
      Response::ReopenExternal(outcome) => {
        if matches!(outcome, InstallOutcome::Installed) {
          self.signal_table_prewarm(tbl);
        }
        Ok(outcome)
      }
      _ => Err(KvError::Corrupted),
    }
  }

  /// 文件 GC：Cloud 删远端对象（并清本地 cache），Local / Mem 删本地文件。
  /// 对齐 C++ `FileGcRequest`；布局位宽由 shard 配置决定，调用方不需要传。
  pub async fn file_gc(&self, tbl: &TableIdent) -> Result<usize> {
    match self
      .request(tbl, Request::FileGc { tbl: tbl.clone() })
      .await?
    {
      Response::FileGc(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 本地 GC：只删本地文件 / 本地 cache，绝不删远端对象。
  /// 对齐 C++ `LocalGcRequest`。
  pub async fn local_gc(&self, tbl: &TableIdent) -> Result<usize> {
    match self
      .request(tbl, Request::LocalGc { tbl: tbl.clone() })
      .await?
    {
      Response::LocalGc(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn compact(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    pages_per_file_shift: u8,
  ) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::Compact {
          tbl: tbl.clone(),
          file_id,
          pages_per_file_shift,
        },
      )
      .await?
    {
      Response::Compact(count) => Ok(count),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn truncate(&self, tbl: &TableIdent) -> Result<()> {
    self.truncate_from(tbl, b"").await
  }

  pub async fn truncate_from(&self, tbl: &TableIdent, position: &[u8]) -> Result<()> {
    match self
      .request(
        tbl,
        Request::Truncate {
          tbl: tbl.clone(),
          position: position.to_vec(),
        },
      )
      .await?
    {
      Response::Truncate => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 触发该表 TTL 树的物理清理（对齐 C++ `CleanExpired` 内部请求）。
  pub async fn clean_expired(&self, tbl: &TableIdent) -> Result<CleanExpiredReply> {
    match self
      .request(tbl, Request::CleanExpired { tbl: tbl.clone() })
      .await?
    {
      Response::CleanExpired(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn drop_table(&self, tbl: &TableIdent) -> Result<()> {
    match self
      .request(tbl, Request::Drop { tbl: tbl.clone() })
      .await?
    {
      Response::Drop => Ok(()),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn create_archive(
    &self,
    tbl: &TableIdent,
    tag: &str,
    num_retained: u32,
  ) -> Result<ArchiveReply> {
    match self
      .request(
        tbl,
        Request::CreateArchive {
          tbl: tbl.clone(),
          tag: tag.to_owned(),
          num_retained,
        },
      )
      .await?
    {
      Response::CreateArchive(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 按 tag 重新打开归档快照（`tag` 为空 = 重开活跃快照）。
  ///
  /// 空 tag 与 [`Self::reopen`] 同一条 C++ 路径（`ReopenRequest` 未设 tag），
  /// Cloud / StandbyReplica 下同样把缺失收敛为成功。
  ///
  /// 非空 tag 是**有意差异**（见 eloqstore2fkv/COMPAT_BASELINE「缺失快照的
  /// 重开语义」）：C++ `ReopenRequest(tag)` 在归档不存在时清本地状态并回
  /// `NoError`（`InstallExternalSnapshot` → `cleared_local_state`）；fkv 回
  /// `ResourceMissing` 且不动现有状态 —— tag 指向确定的历史快照，缺失是「请求
  /// 的状态不存在」，静默清空活跃 manifest 会让嵌入式调用方无从察觉。需要
  /// C++ 清理语义的全局路径（standby `global_reopen(tag)`）走
  /// `Request::ReopenArchive { clean: true }`，行为仍与 C++ 一致。
  pub async fn reopen_archive(&self, tbl: &TableIdent, tag: &str) -> Result<()> {
    match self
      .request(
        tbl,
        Request::ReopenArchive {
          tbl: tbl.clone(),
          tag: tag.to_owned(),
          clean: false,
        },
      )
      .await
    {
      Ok(Response::Reopen) => Ok(()),
      Err(KvError::NotFound | KvError::ResourceMissing)
        if tag.is_empty() && self.remote_state_authoritative() =>
      {
        Ok(())
      }
      Err(err) => Err(err),
      Ok(_) => Err(KvError::Corrupted),
    }
  }

  /// 删除该分区当前分支下指定 tag 的归档 manifest，返回删除条数。
  pub async fn delete_archive(&self, tbl: &TableIdent, tag: &str) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::DeleteArchive {
          tbl: tbl.clone(),
          tag: tag.to_owned(),
        },
      )
      .await?
    {
      Response::DeleteArchive(removed) => Ok(removed),
      _ => Err(KvError::Corrupted),
    }
  }

  /// 列举该表的归档 `(tag, term)` 清单（全部 branch / term，按 `(tag, term)`
  /// 升序去重，对齐 C++ `GlobalListArchiveTagsRequest` 的排序与去重规则）。
  pub async fn list_archive_tags(&self, tbl: &TableIdent) -> Result<Vec<ArchiveTagInfo>> {
    match self
      .request(tbl, Request::ListArchiveTags { tbl: tbl.clone() })
      .await?
    {
      Response::ListArchiveTags(tag_li) => Ok(tag_li),
      _ => Err(KvError::Corrupted),
    }
  }

  /// Cloud 模式：分页列举 store root 下的远端对象。
  ///
  /// `prefix` **相对 store root**（`KvOption::cloud_store_path` 的
  /// `<bucket>/<prefix>` 中后一段），对齐 C++ `ListObjectRequest::SetRemotePath`：
  /// 内部拼成 `<store root>/<prefix>` 后下推，空串列 store root（store root 为空
  /// 时才是整个 bucket），前导 `/` 忽略。例如要列 `demo` 分区 1 的全部对象，
  /// 传 `demo.1/`（不是 `fkv/demo.1/`）。`tbl` 只用于选择服务 shard。
  ///
  /// 返回的 `ObjectMeta::key` 是**相对 `prefix` 的名字**（对齐 C++
  /// `StripPrefixFromKey`），不是完整 bucket key；`next_token` 是内部续页标记，
  /// 原样回传即可，不要解析。
  ///
  /// `recursive` 对齐 C++ `ListObjectRequest::SetRecursive`（默认 `false`）：
  /// `false` 只列当前层对象与 common prefixes，`true` 列全部后代。
  /// 非 Cloud 模式返回 `InvalidArgs`。
  pub async fn list_object(
    &self,
    tbl: &TableIdent,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
    recursive: bool,
  ) -> Result<ListPage> {
    match self
      .request(
        tbl,
        Request::ListObject {
          tbl: tbl.clone(),
          prefix: prefix.to_owned(),
          token: token.map(str::to_owned),
          max_keys,
          recursive,
        },
      )
      .await?
    {
      Response::ListObject(page) => Ok(page),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn create_branch(&self, tbl: &TableIdent, branch: &str) -> Result<BranchReply> {
    match self
      .request(
        tbl,
        Request::CreateBranch {
          tbl: tbl.clone(),
          branch: branch.to_owned(),
        },
      )
      .await?
    {
      Response::CreateBranch(reply) => Ok(reply),
      _ => Err(KvError::Corrupted),
    }
  }

  pub async fn delete_branch(&self, tbl: &TableIdent, branch: &str) -> Result<usize> {
    match self
      .request(
        tbl,
        Request::DeleteBranch {
          tbl: tbl.clone(),
          branch: branch.to_owned(),
        },
      )
      .await?
    {
      Response::DeleteBranch(removed) => Ok(removed),
      _ => Err(KvError::Corrupted),
    }
  }
}
