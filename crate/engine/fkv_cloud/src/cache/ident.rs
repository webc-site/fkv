//! 本地副本的内容归属：写入 identity、per-file 归属链解析与读命中校验。

use fkv_types::{FileId, TableIdent};

use crate::{CloudStore, object_key::range_ident};

impl<I, O> CloudStore<I, O> {
  /// 当前 attach identity 的 owned 形式：写入 / 上传内容的归属。
  pub(crate) fn current_ident(&self) -> (String, u64) {
    let id = self.identity();
    (id.branch.clone(), id.term)
  }

  /// 由 manifest 的 per-file 归属链解析 file id 的写时 identity（owned）。
  ///
  /// 链为空 / file id 超出链尾（当前 term 新分配）时回退当前 identity，
  /// 与 [`Self::ranged_name`] 的远端 key 解析口径一致。
  pub(crate) fn attr_ident(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    segment: bool,
  ) -> (String, u64) {
    let id = self.identity();
    let range_map = self.file_range_map.borrow();
    range_map
      .get(tbl)
      .and_then(|range_li| range_ident(range_li, segment, file_id))
      .map_or_else(
        || (id.branch.clone(), id.term),
        |(branch, term)| (branch.to_owned(), term),
      )
  }

  /// 回填 / 整写覆盖本地内容后改写记账里的内容归属。
  ///
  /// 与 [`Self::upsert_resident`] 的区别：后者在条目已存在时保留原归属
  /// （登记不改变内容）；本函数用于「本地字节刚被整体替换」的路径
  /// （segment 回填、整文件写）。
  pub(crate) fn set_ident(
    &self,
    tbl: &TableIdent,
    file_id: FileId,
    segment: bool,
    ident: (String, u64),
  ) {
    let mut state = self.state.borrow_mut();
    let Some(cache) = state.table_map.get_mut(tbl) else {
      return;
    };
    if segment {
      if let Some(entry) = cache.segment.get_mut(&file_id) {
        entry.ident = Some(ident);
      }
    } else if let Some(entry) = cache.data.get_mut(&file_id) {
      entry.ident = Some(ident);
    }
  }

  /// 冷启动扫盘登记的本地副本内容归属未知（`ident == None`）：按刚下发的
  /// per-file 归属链补记。
  ///
  /// [`Self::restore_table_cache`] 在 replay 之前执行，此刻还没有归属链，
  /// 只能记 `None`；若这里不补记，正常重启（尤其是 term 抬升后）所有旧文件
  /// 都会被读时校验判成 miss，整份 cache 回源。补记后仍保持 io-12 的语义：
  /// 归属一旦变化，记账里的 identity 与实际归属不一致，读路径照样回源。
  ///
  /// 只补记干净副本：脏文件的内容由本进程写入，归属由写路径维护。
  pub(crate) fn adopt_unknown_ident(&self, tbl: &TableIdent) {
    let id = self.identity();
    let range_map = self.file_range_map.borrow();
    let range_li = range_map.get(tbl).map(Vec::as_slice);
    let mut state = self.state.borrow_mut();
    let Some(cache) = state.table_map.get_mut(tbl) else {
      return;
    };
    let ident_of = |file_id: FileId, segment: bool| {
      range_li
        .and_then(|range_li| range_ident(range_li, segment, file_id))
        .map_or_else(
          || (id.branch.clone(), id.term),
          |(branch, term)| (branch.to_owned(), term),
        )
    };
    for (file_id, entry) in cache.data.iter_mut() {
      if entry.ident.is_none() && !entry.dirty {
        entry.ident = Some(ident_of(*file_id, false));
      }
    }
    for (file_id, entry) in cache.segment.iter_mut() {
      if entry.ident.is_none() && !entry.dirty {
        entry.ident = Some(ident_of(*file_id, true));
      }
    }
  }

  /// 本地 data / segment 文件的内容归属是否就是读路径要求的 `(branch, term)`。
  ///
  /// 读命中前必须过这一关（io-12）：本地 cache 只按 file id 记账，跨 term
  /// 重开归档时同一 file id 指向另一个 term 的远端对象，旧副本不能命中。
  ///
  /// - 条目不存在（未登记 / 已淘汰）→ 不是可信命中；
  /// - 脏文件是本地唯一副本（回源会丢未上传写）→ 沿用旧行为，信任本地；
  /// - 干净副本要求记账里的写时 identity 与 manifest 的 per-file 归属一致。
  pub(crate) fn local_ident_ok(&self, tbl: &TableIdent, file_id: FileId, segment: bool) -> bool {
    let state = self.state.borrow();
    let Some((dirty, ident)) = state.table_map.get(tbl).and_then(|cache| {
      if segment {
        cache
          .segment
          .get(&file_id)
          .map(|entry| (entry.dirty, entry.ident.as_ref()))
      } else {
        cache
          .data
          .get(&file_id)
          .map(|entry| (entry.dirty, entry.ident.as_ref()))
      }
    }) else {
      return false;
    };
    if dirty {
      return true;
    }
    let id = self.identity();
    let range_map = self.file_range_map.borrow();
    let (branch, term) = range_map
      .get(tbl)
      .and_then(|range_li| range_ident(range_li, segment, file_id))
      .unwrap_or((id.branch.as_str(), id.term));
    ident.is_some_and(|(entry_branch, entry_term)| {
      entry_branch.as_str() == branch && *entry_term == term
    })
  }
}
