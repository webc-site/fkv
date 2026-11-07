//! manifest 上传 / 下载 / 校验 / 重试 / 剪枝与远端 manifest 列举。

use std::time::Duration;

use bytes::Bytes;
use fkv_error::{KvError, Result};
use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::TableIdent;
use futures_timer::Delay;
use gxhash::HashSet;

use crate::{CloudStore, group::PublishGuard, listing::CLOUD_LIST_PAGE_KEYS};

/// manifest 上传的最大尝试次数（含首次）。
pub(crate) const MANIFEST_RETRY_TIMES: u32 = 10;
/// manifest 重试首次退避的毫秒数。
pub(crate) const MANIFEST_RETRY_INIT_DELAY_MS: u64 = 10;
/// manifest 重试退避的毫秒上限。
pub(crate) const MANIFEST_RETRY_MAX_DELAY_MS: u64 = 1000;

/// manifest 重试判据：通用瞬时错误（`is_retryable`）之上，额外接纳超时与云服务错误。
/// 云端不可用表现为 `Timeout` / `CloudErr`，不属于进程内瞬时错误分类。
pub(crate) const fn manifest_retryable(err: KvError) -> bool {
  err.is_retryable() || matches!(err, KvError::Timeout | KvError::CloudErr)
}

impl<I: FileIo + ManifestIo, O: ObjectStore> CloudStore<I, O> {
  /// manifest 是 durability anchor：瞬态错误重试到成功 / 空间不足，
  /// 对齐 C++ `SyncFile` 的 `while (err != NoError && err != OssInsufficientStorage)`。
  ///
  /// `bytes` 移交所有权：重试只是 `Bytes` 引用计数 +1，不会每次重发前再复制一遍负载。
  pub(crate) async fn put_manifest_with_retry(&self, key: &str, bytes: Bytes) -> Result<()> {
    let mut delay_ms = MANIFEST_RETRY_INIT_DELAY_MS;
    for attempt in 0..MANIFEST_RETRY_TIMES {
      // 测试注入：模拟远端 manifest 发布持续失败，验证本地追加回滚。
      fkv_failpoint::fail_point!(CLOUD_MANIFEST_PUT);
      match self.remote.put(key, bytes.clone(), None, false).await {
        Ok(_) => return Ok(()),
        Err(err) if manifest_retryable(err) && attempt + 1 < MANIFEST_RETRY_TIMES => {
          Delay::new(Duration::from_millis(delay_ms)).await;
          delay_ms = (delay_ms * 2).min(MANIFEST_RETRY_MAX_DELAY_MS);
        }
        Err(err) => return Err(err),
      }
    }
    Err(KvError::CloudErr)
  }
}

impl<I: ManifestIo + PageIo + SegmentIo + FileIo, O: ObjectStore> ManifestIo for CloudStore<I, O> {
  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    // 先上传 data 文件，再让 manifest 指向它们。
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    // 本地 cache 不是 append 的可靠基线：首写窗口 / 被 prewarm 删除后本地
    // 可能为空，而远端已有完整快照。先回源补齐，仍无 manifest 则拒绝 append
    // （否则单条 log 会成为远端首记录，reopen 直接 EndOfFile）。
    let mut old = self.local.read_manifest(tbl).await?;
    if old.is_none() {
      self.read_manifest(tbl).await?;
      old = self.local.read_manifest(tbl).await?;
    }
    // 上面已补齐本地 manifest，缺它就是远端也没有：拒绝 append。
    let old_len = old.as_ref().ok_or(KvError::ResourceMissing)?.len() as u64;
    self.local.append_manifest(tbl, record).await?;
    let Some(bytes) = self.local.read_manifest(tbl).await? else {
      self.local.reset_manifest(tbl, old_len).await?;
      return Err(KvError::ResourceMissing);
    };
    if let Err(err) = self
      .put_manifest_with_retry(&self.manifest_key(tbl), Bytes::from(bytes))
      .await
    {
      // 远端发布失败：本地追加必须回滚，否则下一次成功的 append 会把这条
      // 未提交 record 一并上传；其 FilePageId 已被 allocator 回滚并被后续
      // 批次复用，reopen 会按错误 mapping 读到别页数据。
      let rollback = self.local.reset_manifest(tbl, old_len).await;
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "manifest 上传失败后回滚本地失败 tbl={tbl} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    let old = self.local.read_manifest(tbl).await?;
    self.local.switch_manifest(tbl, record).await?;
    if let Err(err) = self
      .put_manifest_with_retry(&self.manifest_key(tbl), Bytes::copy_from_slice(record))
      .await
    {
      // 远端发布失败：本地已切到新快照，必须切回旧快照；否则下一次写以
      // 这份未提交快照为基线追加 log 并上传，把失败的 truncate / archive
      // 提升成远端权威。
      let rollback = match old {
        Some(bytes) => self.local.switch_manifest(tbl, &bytes).await,
        None => self.local.drop_manifest(tbl).await,
      };
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "switch manifest 上传失败后回滚本地失败 tbl={tbl} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    // 活跃 manifest 的追加基准：replay 后把本地 cache 截到最后一条有效 record。
    // 远端对象是整文件 `put` 的权威快照，不在 reopen 热路径上重发；若远端本身
    // 带撕裂 / 垃圾尾（旧版本上传或从撕裂 cache 恢复），截断本地后下一次
    // append 的整文件 put 会自然把它修复成干净快照。
    self.local.reset_manifest(tbl, size).await
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    // branch / archive 具名 manifest 同活跃 manifest 契约：只截断本地 cache。
    // `read_manifest_file` 读取时以远端完整快照覆写本地；但远端对象也可能
    // 携带撕裂尾（旧版本把撕裂 cache 整份上传，或人工恢复），此时 replay 返回
    // 的 `valid_len` 小于本地文件长度，必须在这里把本地拉回有效边界，否则下一次
    // `append_manifest_file` 会把垃圾尾连同新 record 一起 put 回远端，永久固化。
    // 不回写远端：reopen 热路径不做网络写，且完整 put 由下一次 append 自然完成。
    self.local.reset_manifest_file(tbl, name, size).await
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    // 冷启动第一次打开该表：先按本地物理文件重建 cache 记账（按表懒，仅一次）。
    // 否则旧进程留下的文件不计预算、不参与 LRU，还会被预热当成 miss。
    self.restore_table_cache(tbl).await?;
    let local = self.local.read_manifest(tbl).await?;
    // 远端是权威（对齐 C++ `PageManager` Cloud 路径无条件 `RefreshManifest`）：
    // 本地命中也要回源。本地 manifest 文件名与 term 无关（恒为 `manifest_main_0`），
    // 跨 term 接管复用旧目录时会命中上一任的 stale 快照；若直接以它为 replay 基线，
    // 不仅会读到过期 root，后续写回还会把它晋升成远端权威（rev-34）。
    //
    // 选择结果带采纳 term 栅栏（sem-04，对齐 C++ `GetManifest`）：选中 term
    // 高于本 shard term 说明更高 term 的 owner 已接管，本实例是过期 owner。
    let remote = self
      .latest_remote_manifest_fenced(tbl)
      .await?
      .map(|(_, bytes)| bytes);
    match (local, remote) {
      // 本地 cache 与远端当前快照一致：无需改写本地。
      (Some(local), Some(remote)) if remote.as_ref() == local.as_slice() => Ok(Some(local)),
      // 本地缺失或落后：采纳远端并以本地 cache 覆写，避免下一次命中 stale。
      (_, Some(remote)) => {
        self.local.switch_manifest(tbl, &remote).await?;
        Ok(Some(remote.into()))
      }
      // 远端没有 manifest：本地只是 cache，不能继续作为权威基线
      // （对齐 C++ NotFound → `DropLocalManifest`）。返回 None 让 reopen 走
      // ResourceMissing / 重试队列，绝不以 stale 快照懒创建。
      (Some(_), None) => {
        self.local.drop_manifest(tbl).await?;
        Ok(None)
      }
      (None, None) => Ok(None),
    }
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    // 本地 cache 删除失败不能阻断远端 durability anchor 的删除；收集首错，
    // 继续清标记与远端 manifest。
    let mut first_err = match self.local.drop_manifest(tbl).await {
      Ok(()) => None,
      Err(err) => {
        log::warn!("drop 删除本地 manifest 失败 tbl={tbl}: {err}");
        Some(err)
      }
    };
    // 清掉 segment 旁路标记：残留标记会让分区目录永不回收，也会在 file_id
    // 复用时把新的半装文件误判完整。失败不阻断后续远端 manifest 删除。
    if let Err(err) = self.clear_segment_marker_li(tbl).await {
      log::warn!("drop 清理 segment 标记失败 tbl={tbl}: {err}");
      first_err.get_or_insert(err);
    }
    // 活跃 manifest 名带**写时 term**（`append_manifest` / `switch_manifest`
    // 都走 [`Self::manifest_key`]）。attach 抬高 term 后本 term 可能从未写过该
    // 表，远端权威 manifest 仍在被接管的旧 term；只删 identity 名会落空，
    // reopen 按 `latest_remote_manifest` 又选中它 → 表永久复活。
    //
    // 因此删除**同 branch、无 tag、term 不高于本 identity 的全部** manifest：
    // 只删最新一条会留下更旧 term 的 durability anchor（`manifest{1,3}` +
    // drop@term3 删掉 3 后 reopen 会选中 1）；更高 term 属于新 owner，保留其
    // durability anchor（term fencing）。
    let (branch, term) = {
      let id = self.identity();
      (id.branch.clone(), id.term)
    };
    let own = fkv_types::manifest_file_name(&branch, term);
    let mut name_li: Vec<(u64, String)> = self
      .remote_manifest_name_li(tbl)
      .await?
      .into_iter()
      .filter_map(|name| {
        let (file_branch, file_term, tag) = fkv_types::parse_manifest_file_name(&name)?;
        (tag.is_none() && file_branch == branch.as_str() && file_term <= term)
          .then_some((file_term, name))
      })
      .collect();
    // 本 term 可能从未写过该表：own 名不在远端列举里也要幂等删一次。
    if !name_li.iter().any(|(_, name)| name == &own) {
      name_li.push((term, own));
    }
    // 旧 term 先删、own（本 identity 的最高 term）最后。反过来先删 own，并发
    // 读者会在两次删除之间只看到旧 manifest，采纳后回写把它晋升成新权威；
    // own 最后时中间窗口最多采纳 own 本身，其回写要与最后一步删除竞争。
    name_li.sort_unstable();
    for (_, name) in name_li {
      let key = self.manifest_file_key(tbl, &name);
      // 单条失败不能中断：否则 own 留在远端，reopen 会把已 drop 的表复活。
      // 收集首个错误，继续删到 own，最后统一返回。
      if let Err(err) = self.remote.delete(&key).await {
        log::warn!("drop 删除远端 manifest 失败 tbl={tbl} name={name}: {err}");
        first_err.get_or_insert(err);
        continue;
      }
      // 远端已删；本地同名缓存清理失败不影响删除结果。
      if let Err(err) = self.local.remove_manifest_file(tbl, &name).await {
        log::warn!("本地 manifest 缓存清理失败 tbl={tbl} name={name}: {err}");
      }
    }
    match first_err {
      Some(err) => Err(err),
      None => Ok(()),
    }
  }

  /// 无条件回源远端 active manifest 覆盖本地 cache（采纳外部换主后的状态）。
  ///
  /// 与 [`Self::read_manifest`] 的区别：本地命中也要刷新。远端缺失返回
  /// `ResourceMissing`，交给 `Store::install_external_snapshot` 清本地状态。
  ///
  /// **不做采纳 term 栅栏**（sem-04）：对齐 C++ `RefreshManifest`（reopen /
  /// `InstallExternalSnapshot` 路径）——它是外部换主的显式入口，低 term 的
  /// follower / standby 就是要采纳更高 term 的新 owner 快照；对这条路径加栅栏
  /// 会拒掉合法的交接。调用方随后用 [`ManifestIo::read_local_manifest`] 回放
  /// 刚写入本地 cache 的快照，不再按远端最新 term 重选。
  async fn refresh_manifest(&self, tbl: &TableIdent) -> Result<()> {
    // 与 `read_manifest` 同一选择规则：外部换主可能把 manifest 写到更高 term，
    // 固定拼当前 identity 会永远读不到。但这条路径不做 term 栅栏（见文档）。
    let bytes = self.latest_remote_manifest_bytes(tbl).await?;
    // 外部状态可能追加 / 重排文件：无论远端是否存在，本地 cache 都必须作废，
    // 否则旧文件会被当作有效 cache 跳过远端（远端缺失时也要清，等待新状态）。
    self.invalidate_local_file_li(tbl).await?;
    let Some(bytes) = bytes else {
      return Err(KvError::ResourceMissing);
    };
    self.local.switch_manifest(tbl, &bytes).await?;
    Ok(())
  }

  /// 只删本地 cache，不触碰远端 manifest（对齐 C++ `DropLocalManifest`）。
  async fn drop_local_manifest(&self, tbl: &TableIdent) -> Result<()> {
    self.local.drop_manifest(tbl).await
  }

  /// 只读本地 cache 的 active manifest（不列举远端、不做 term 栅栏）。
  ///
  /// 供 `Store::install_external_snapshot` 在 [`ManifestIo::refresh_manifest`]
  /// 已把外部快照写入本地 cache 后回放：等价 C++ `RefreshManifest` 的
  /// 「下载到 buffer → replay → 写本地 → 打开本地」，不能再次回源选择
  /// （否则会重新触发 `GetManifest` 侧的采纳 term 栅栏）。
  async fn read_local_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    self.restore_table_cache(tbl).await?;
    self.local.read_manifest(tbl).await
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    // 分支 manifest 与活跃 manifest 同契约：先落数据，再发布引用它的 manifest。
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    // 本地具名 manifest 缺失时先回源；远端也没有就拒绝 append，避免单条 log
    // 成为 branch manifest 首记录。branch 活跃名还要走回退链（sem-60 写时
    // 晋升）：首写发布前远端只有旧 term 对象，当前名 miss 时按同 branch 最高
    // 无 tag term 采纳并重新装种子；归档名保持精确名回源。
    let mut old = self.local.read_manifest_file(tbl, name).await?;
    if old.is_none() {
      let (branch, term) = {
        let id = self.identity();
        (id.branch.clone(), id.term)
      };
      if *name == fkv_types::manifest_file_name(&branch, term) {
        self.read_branch_manifest(tbl, &branch, term).await?;
      } else {
        self.read_manifest_file(tbl, name).await?;
      }
      old = self.local.read_manifest_file(tbl, name).await?;
    }
    let Some(ref old_bytes) = old else {
      return Err(KvError::ResourceMissing);
    };
    let old_len = old_bytes.len() as u64;
    self.local.append_manifest_file(tbl, name, record).await?;
    let Some(bytes) = self.local.read_manifest_file(tbl, name).await? else {
      self.local.reset_manifest_file(tbl, name, old_len).await?;
      return Err(KvError::ResourceMissing);
    };
    if let Err(err) = self
      .put_manifest_with_retry(&self.manifest_file_key(tbl, name), Bytes::from(bytes))
      .await
    {
      let rollback = match old {
        Some(_) => self.local.reset_manifest_file(tbl, name, old_len).await,
        None => self.local.remove_manifest_file(tbl, name).await,
      };
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "branch manifest 上传失败后回滚本地失败 tbl={tbl} name={name} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    self.flush_all_tbl(tbl).await?;
    let _publish = PublishGuard::new(&self.owner.group, tbl);
    let old = self.local.read_manifest_file(tbl, name).await?;
    self.local.write_manifest_file(tbl, name, record).await?;
    if let Err(err) = self
      .put_manifest_with_retry(
        &self.manifest_file_key(tbl, name),
        Bytes::copy_from_slice(record),
      )
      .await
    {
      // 远端发布失败：本地具名快照（branch / archive）必须回滚，否则下一次
      // append 会以失败快照为基线整份上传，把 abort 的批次 / truncate 转正。
      let rollback = match old {
        Some(bytes) => self.local.write_manifest_file(tbl, name, &bytes).await,
        None => self.local.remove_manifest_file(tbl, name).await,
      };
      if let Err(rollback_err) = rollback {
        log::warn!(
          target: "fkv_cloud",
          "具名 manifest 上传失败后回滚本地失败 tbl={tbl} name={name} err={rollback_err}"
        );
      }
      return Err(err);
    }
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    // 测试注入：模拟云端 ListObjects 网络错误（sem-63，drop 列举失败路径）。
    fkv_failpoint::fail_point!(CLOUD_LIST_MANIFEST_FAIL);
    // 远端是 manifest 名单的唯一权威：本地 cache 可能残留失败发布 / 崩溃前
    // 写入的「幻影」具名 manifest，合并列举会让 archive reopen 选中幻影、
    // 让 prune 多删真实归档。segment 完成标记属旁路 sidecar，不在此列。
    let mut name_li: Vec<String> = self.remote_manifest_name_li(tbl).await?;
    name_li.sort();
    Ok(name_li)
  }

  /// 旁路文件只在本地 cache，绝不上传远端；必须显式下探本地后端。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    self.local.list_sidecar_file_name_li(tbl).await
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    self.local.remove_sidecar_file_name(tbl, name).await
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    // branch / archive 打开同样先恢复该表的记账。
    self.restore_table_cache(tbl).await?;
    // 具名 manifest 也以远端为权威：本地 cache 可能属于旧 term / 旧进程，
    // 命中后直接返回会让 reopen 读到 stale 前缀，下一次 append 再把它整份
    // 发布回远端，永久覆盖别的主机已提交的 record。
    let Some(bytes) = self.remote.get(&self.manifest_file_key(tbl, name)).await? else {
      // 远端已删：本地 cache 同步删除，避免 archive/branch 被“复活”。
      self.local.remove_manifest_file(tbl, name).await?;
      return Ok(None);
    };
    let local = self.local.read_manifest_file(tbl, name).await?;
    if local.as_deref() != Some(bytes.as_ref()) {
      self.local.write_manifest_file(tbl, name, &bytes).await?;
      // 覆盖整文件后重置追加偏移，后续 append 从新长度继续。
      self
        .local
        .reset_manifest_file(tbl, name, bytes.len() as u64)
        .await?;
    }
    Ok(Some(bytes.into()))
  }

  /// branch manifest 晋升种子：只装本地 cache，不发布远端对象（sem-60 写时
  /// 晋升——首写 `append_manifest_file` 才按当前 identity 名上传）。
  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> Result<()> {
    self.local.write_manifest_file(tbl, name, bytes).await?;
    // 覆盖整文件后重置追加偏移，与 `read_manifest_file` 的 cache 装载同口径。
    self
      .local
      .reset_manifest_file(tbl, name, bytes.len() as u64)
      .await
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let local_err = self.local.remove_manifest_file(tbl, name).await.err();
    let remote_result = self.remote.delete(&self.manifest_file_key(tbl, name)).await;
    match (local_err, remote_result) {
      (_, Err(err)) => Err(err),
      (Some(err), Ok(())) => Err(err),
      (None, Ok(())) => Ok(()),
    }
  }

  /// 剪枝被同表当前 term 取代的远端 manifest 对象。
  ///
  /// 保守约束（任一不满足即整轮跳过）：
  /// - `term > 0`（无更旧 term 可删；`main` 在云端同样按 term 命名，参与剪枝）；
  /// - 本实例 identity 与调用参数一致，避免替别的 branch / term 做判定；
  /// - 远端同 branch 无 tag manifest 的最高 term 必须等于 `term`，出现更高 term
  ///   说明另有实例在推进，本轮不删任何对象；
  /// - 只删 `term` 更小者，当前 term 对象永不删除。
  ///
  /// 删除顺序为远端对象 → 本地 cache；任一失败只记录日志，不阻断 GC 收尾。
  async fn prune_superseded_manifest_file_li(
    &self,
    tbl: &TableIdent,
    branch: &str,
    term: u64,
  ) -> Result<usize> {
    if term == 0 {
      return Ok(0);
    }
    {
      let id = self.identity();
      if id.branch != branch || id.term != term {
        return Ok(0);
      }
    }
    // 只信远端真实现状：本地 cache 里的残留名字不能当作对象存在的证据。
    let name_li = self.remote_manifest_name_li(tbl).await?;
    let mut term_li: Vec<(u64, &str)> = name_li
      .iter()
      .filter_map(|name| {
        let (file_branch, file_term, tag) = fkv_types::parse_manifest_file_name(name)?;
        (tag.is_none() && file_branch == branch).then_some((file_term, name.as_str()))
      })
      .collect();
    term_li.sort_unstable_by_key(|(file_term, _)| *file_term);
    // 更高 term 存在时放弃：旧读者可能仍在旧 term 上按 manifest 判定引用集。
    if term_li.last().map(|(file_term, _)| *file_term) != Some(term) {
      return Ok(0);
    }
    let mut removed = 0;
    for (file_term, name) in term_li {
      if file_term >= term {
        continue;
      }
      let key = self.manifest_file_key(tbl, name);
      if let Err(err) = self.remote.delete(&key).await {
        log::warn!("远端 manifest 剪枝失败 tbl={tbl} name={name}: {err}");
        continue;
      }
      removed += 1;
      // 远端已删，本地缓存清理失败不影响结果：下次列举仍以远端为准。
      if let Err(err) = self.local.remove_manifest_file(tbl, name).await {
        log::warn!("本地 manifest 缓存清理失败 tbl={tbl} name={name}: {err}");
      }
    }
    Ok(removed)
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 分页列举远端 manifest 文件名（不合并本地 cache），升序返回。
  pub(crate) async fn remote_manifest_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut name_set = HashSet::default();
    let mut token = None;
    loop {
      let page = self
        .remote
        .list_page(
          &self.manifest_scan_prefix(tbl),
          token.as_deref(),
          CLOUD_LIST_PAGE_KEYS,
        )
        .await?;
      for meta in page.meta_li {
        let Some(name) = meta.key.rsplit('/').next() else {
          continue;
        };
        if fkv_types::parse_manifest_file_name(name).is_some() {
          name_set.insert(name.to_owned());
        }
      }
      match page.next_token {
        Some(next) => token = Some(next),
        None => break,
      }
    }
    let mut name_li: Vec<String> = name_set.into_iter().collect();
    name_li.sort();
    Ok(name_li)
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 选出远端当前 branch 的最新 term manifest 并读取内容（**无**采纳栅栏）。
  ///
  /// 跨 term 写入后 manifest 对象名带写时 term，只按进程 identity 拼 key 会
  /// 看不到旧 / 新 term 的对象。这里列举同 branch 全部无 tag manifest，取最大
  /// term；term 与进程不同的对齐交给 `Store::install_replay_state`（跨 term bump
  /// allocator）完成 —— cloud 层不改进程 identity term：Rust 的 term 由 attach
  /// 的 term 文件 CAS 决定，cloud 擅自改写会与 Store / term 文件脱节。
  ///
  /// 调用方是 `refresh_manifest`（外部换主，对齐 C++ `RefreshManifest`，
  /// 该路径无栅栏）；惰性读路径用 [`Self::latest_remote_manifest_fenced`]。
  pub(crate) async fn latest_remote_manifest_bytes(
    &self,
    tbl: &TableIdent,
  ) -> Result<Option<Bytes>> {
    Ok(
      self
        .latest_remote_manifest(tbl)
        .await?
        .map(|(_, bytes)| bytes),
    )
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 选远端当前 branch 的最新 term manifest，返回**对象名 + 内容**。
  ///
  /// 不做采纳 term 栅栏：调用方是 `refresh_manifest`（对齐 C++ `RefreshManifest`
  /// 的外部换主路径）与预热。惰性读路径必须用
  /// [`Self::latest_remote_manifest_fenced`]。
  pub(crate) async fn latest_remote_manifest(
    &self,
    tbl: &TableIdent,
  ) -> Result<Option<(String, Bytes)>> {
    let Some(name) = self.latest_remote_manifest_name(tbl).await? else {
      return Ok(None);
    };
    let key = self.manifest_file_key(tbl, &name);
    Ok(self.remote.get(&key).await?.map(|bytes| (name, bytes)))
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 惰性读路径的远端 active manifest 选择 + 采纳 term 栅栏（对齐 C++
  /// `CloudStoreMgr::GetManifest`，sem-04）。
  ///
  /// 回退选择出的无 tag manifest term 高于本 shard term 时，说明更高 term 的
  /// owner 已接管该分区：本实例是过期 owner，返回 [`KvError::ExpiredTerm`]，
  /// **不下载、不采纳**（本地 cache 保持原状）。shard 侧收到该错误后把整个
  /// `DsShardState` 置 `Expired`，后续请求快速失败（复用写路径 term 校验的
  /// 通道）。`selected_term <= self.term()` 保持原有跨 term 采纳旧快照行为。
  ///
  /// 只覆盖 `GetManifest` 等价路径；`refresh_manifest`（对齐 `RefreshManifest`）
  /// 与 `prewarm_manifest`（只读预热，不承担 owner 身份）不做栅栏。
  pub(crate) async fn latest_remote_manifest_fenced(
    &self,
    tbl: &TableIdent,
  ) -> Result<Option<(String, Bytes)>> {
    let Some(name) = self.latest_remote_manifest_name(tbl).await? else {
      return Ok(None);
    };
    // `latest_remote_manifest_name` 已按 manifest 命名规则解析过，这里重取 term。
    let selected_term = fkv_types::parse_manifest_file_name(&name)
      .ok_or(KvError::Corrupted)?
      .1;
    if selected_term > self.identity().term {
      return Err(KvError::ExpiredTerm);
    }
    let key = self.manifest_file_key(tbl, &name);
    Ok(self.remote.get(&key).await?.map(|bytes| (name, bytes)))
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 选远端当前 branch 最新 term 的无 tag manifest 对象名（不发 GET）。
  pub(crate) async fn latest_remote_manifest_name(
    &self,
    tbl: &TableIdent,
  ) -> Result<Option<String>> {
    // `Ref` 不跨 await 持有。
    let branch = self.identity().branch.clone();
    Ok(
      self
        .remote_manifest_name_li(tbl)
        .await?
        .into_iter()
        .filter_map(|name| {
          let (file_branch, file_term, tag) = fkv_types::parse_manifest_file_name(&name)?;
          (tag.is_none() && file_branch == branch).then_some((file_term, name))
        })
        .max_by_key(|(file_term, _)| *file_term)
        .map(|(_, name)| name),
    )
  }
}
