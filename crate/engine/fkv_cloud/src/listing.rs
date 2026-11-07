//! 远端 data / segment / 分区分页列举。

use fkv_error::Result;
use fkv_io::FileVariant;
use fkv_object::ObjectStore;
use fkv_types::{DATA_FILE_PREFIX, FileId, SEGMENT_FILE_PREFIX, TableIdent};
use gxhash::HashSet;

use crate::{CloudStore, object_key::range_ident};

/// 远端列举每页条数；与 C++ prewarm 的有界队列（1000）同量级，
/// 页内对象只在消费期间驻留，内存 O(页) 而非 O(bucket)。
pub const CLOUD_LIST_PAGE_KEYS: usize = 1000;

/// 远端文件条目：id + 对象字节数。预热要按 `(剩余空间 < 文件大小)` 预判
/// 磁盘是否够用（对齐 C++ `LocalCacheRemained() < file.file_size`），
/// 大小从列举结果直接拿，不额外发 HEAD。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteFileEntry {
  pub file_id: FileId,
  pub size: u64,
}

/// 远端文件条目的一页；`next_token` 语义同 [`ObjectStore::list_page`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePage {
  pub entry_li: Vec<RemoteFileEntry>,
  pub next_token: Option<String>,
}

/// 远端分区（`{tbl}.{partition}`）列举的一页。
///
/// 同一分区的对象在字节序列举里连续，因此跨页去重只需记住「本页处理到的最后
/// 一个分区」(`last`)：下一页把它作为 `after` 传入即可，内存 O(页) 而非 O(bucket)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartitionPage {
  pub table_li: Vec<TableIdent>,
  /// 本页处理到的最后一个分区（含尚未列举完的那个），下一页据此去重。
  pub last: Option<TableIdent>,
  pub next_token: Option<String>,
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 分页列某 table 目录下属于当前 branch 的 data / segment 文件
  /// （id + 对象大小）。
  ///
  /// 当前 identity 的 term 直接收；旧 term 对象只有在该 file id 被 manifest
  /// `file_range_li` 引用时才收（对齐 C++ `FindBranchRange`）。只按
  /// `name_term == term` 过滤会漏掉 branch / 旧 term 区间引用的文件，导致这些
  /// 数据永不预热。
  ///
  /// `token` / `next_token` 语义同 [`ObjectStore::list_page`]，
  /// 调用方逐页消费即可，无需全量物化。
  pub async fn remote_file_entry_page(
    &self,
    tbl: &TableIdent,
    segment: bool,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<FilePage> {
    let kind = if segment {
      SEGMENT_FILE_PREFIX
    } else {
      DATA_FILE_PREFIX
    };
    let page = self
      .remote
      .list_page(&self.tbl_scan_prefix(tbl), token, max_keys)
      .await?;
    // 借用放在 await 之后：`Ref` 不跨 await 持有。
    let id = self.identity();
    let (branch, term) = (id.branch.as_str(), id.term);
    let range_map = self.file_range_map.borrow();
    let range_li = range_map.get(tbl).map_or(&[][..], Vec::as_slice);
    let entry_li = page
      .meta_li
      .into_iter()
      .filter_map(|meta| {
        let name = meta.key.rsplit('/').next()?;
        if !name.starts_with(kind) {
          return None;
        }
        let (file_id, name_branch, name_term) = fkv_types::parse_data_file_name(name)?;
        // 当前 branch + 当前 term 走快路径；其余对象必须是 manifest 区间仍引用的
        // 写时 `(branch, term)` —— 显式放行父 branch 与旧 term，否则子分支继承的
        // 数据永远不会被预热。
        let current = name_branch == branch && name_term == term;
        if !current && range_ident(range_li, segment, file_id) != Some((name_branch, name_term)) {
          return None;
        }
        Some(RemoteFileEntry {
          file_id,
          size: meta.size,
        })
      })
      .collect();
    Ok(FilePage {
      entry_li,
      next_token: page.next_token,
    })
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  pub async fn remote_data_file_page(
    &self,
    tbl: &TableIdent,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<FilePage> {
    self
      .remote_file_entry_page(tbl, false, token, max_keys)
      .await
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  pub async fn remote_segment_file_page(
    &self,
    tbl: &TableIdent,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<FilePage> {
    self
      .remote_file_entry_page(tbl, true, token, max_keys)
      .await
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 分页列举远端已存在的分区（`{tbl}.{partition}`）。
  ///
  /// 用于 Cloud 模式下「远端已有但本地未 cache」的分区枚举：prewarm 的预热目标、
  /// archive crond 的归档对象都以远端为准。`after` 传上一页的 [`PartitionPage::last`]
  /// 做跨页去重，`token` 语义同 [`ObjectStore::list_page`]。
  pub async fn remote_partition_page(
    &self,
    after: Option<&TableIdent>,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<PartitionPage> {
    let prefix = self.prefix.trim_matches('/');
    let scan_prefix = if prefix.is_empty() {
      String::new()
    } else {
      format!("{prefix}/")
    };
    let page = self.remote.list_page(&scan_prefix, token, max_keys).await?;
    let mut table_li = Vec::new();
    let mut last = after.cloned();
    for meta in &page.meta_li {
      let Some(tbl) = self.partition_of_key(&meta.key) else {
        continue;
      };
      if last.as_ref() != Some(&tbl) {
        table_li.push(tbl.clone());
        last = Some(tbl);
      }
    }
    Ok(PartitionPage {
      table_li,
      last,
      next_token: page.next_token,
    })
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 列全量文件 id（内部逐页翻）。大 bucket 下会物化整个 prefix，
  /// prewarm 等流式场景应改用 [`CloudStore::remote_data_file_page`]。
  ///
  /// 云端对象 key 带写时 `(branch, term)`：列举前缀只到 `data_` / `segment_`，
  /// 再解析对象名，**不按当前 identity 的 term 过滤**，否则旧 term 对象永远
  /// 进不了 GC 候选（云盘垃圾只增不减）。branch 仍按当前 identity 归一：
  /// 跨 branch 的对象由各自 branch 的 GC 轮次处理，避免误删父 / 子 branch 数据。
  pub(crate) async fn remote_file_id_li_of(
    &self,
    tbl: &TableIdent,
    segment: bool,
  ) -> Result<Vec<FileId>> {
    let mut variant_li = self.remote_file_variant_li(tbl, segment).await?;
    variant_li.sort_unstable_by_key(|variant| variant.file_id);
    variant_li.dedup_by_key(|variant| variant.file_id);
    Ok(
      variant_li
        .into_iter()
        .map(|variant| variant.file_id)
        .collect(),
    )
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 列远端对象变体（rev-39）：对象名解析出 `(file_id, branch, term)`，branch
  /// 仍按当前 identity 过滤（跨 branch 对象留给各自 branch 的 GC 轮次），
  /// **term 不过滤**——旧 term 对象必须进 GC 候选。升序去重。
  pub(crate) async fn remote_file_variant_li(
    &self,
    tbl: &TableIdent,
    segment: bool,
  ) -> Result<Vec<FileVariant>> {
    let kind = if segment {
      SEGMENT_FILE_PREFIX
    } else {
      DATA_FILE_PREFIX
    };
    let scan_prefix = format!("{}{kind}", self.tbl_scan_prefix(tbl));
    // `Ref` 不跨 await 持有。
    let branch = self.identity().branch.clone();
    let mut variant_set = HashSet::default();
    let mut token: Option<String> = None;
    loop {
      let page = self
        .remote
        .list_page(&scan_prefix, token.as_deref(), CLOUD_LIST_PAGE_KEYS)
        .await?;
      for meta in page.meta_li {
        let Some(name) = meta.key.rsplit('/').next() else {
          continue;
        };
        let Some((file_id, name_branch, name_term)) = fkv_types::parse_data_file_name(name) else {
          continue;
        };
        if name_branch == branch.as_str() {
          variant_set.insert(FileVariant {
            file_id,
            identity: Some((branch.as_str().into(), name_term)),
          });
        }
      }
      match page.next_token {
        Some(next) => token = Some(next),
        None => break,
      }
    }
    let mut variant_li: Vec<FileVariant> = variant_set.into_iter().collect();
    variant_li.sort_unstable();
    Ok(variant_li)
  }
}
