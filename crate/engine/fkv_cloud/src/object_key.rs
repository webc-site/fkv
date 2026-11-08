//! 对象命名 / 路径 / 分区解析：branch+term identity、data / segment / manifest key。

use core::cell::Ref;

use fkv_io::{FileIo, ManifestIo, PageIo, SegmentIo};
use fkv_object::ObjectStore;
use fkv_types::{BranchFileRange, FileId, FilePageId, MAIN_BRANCH_NAME, TableIdent};

use crate::CloudStore;

/// 当前分支 / term，参与云对象文件名。对齐 C++ `ComposeKey`：
/// `<prefix>/<table>.<partition>/<data|segment|manifest>_<...>_<branch>_<term>`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileIdentity {
  pub(crate) branch: String,
  pub(crate) term: u64,
}

impl Default for FileIdentity {
  fn default() -> Self {
    Self {
      branch: MAIN_BRANCH_NAME.to_owned(),
      term: 0,
    }
  }
}

/// 在 `file_range_li` 链中二分定位 file id 的 `(branch, term)`。
///
/// `max_file_id` / `max_segment_file_id` 跨区间单调不减（对齐 C++ `FindBranchRange`
/// 的 `lower_bound`），因此 `partition_point` 对两类文件都成立。
pub(crate) fn range_ident(
  range_li: &[BranchFileRange],
  segment: bool,
  file_id: FileId,
) -> Option<(&str, u64)> {
  let index = if segment {
    range_li.partition_point(|range| range.max_segment_file_id < file_id)
  } else {
    range_li.partition_point(|range| range.max_file_id < file_id)
  };
  range_li
    .get(index)
    .map(|range| (range.branch_name.as_ref(), range.term))
}

impl<I, O> CloudStore<I, O> {
  /// 同步 per-file `(branch, term)` 归属链（对齐 `PageIo::set_file_range_li`）。
  ///
  /// 预热在安装 manifest 后自行解析 branch metadata，需要无 `PageIo` 约束的入口。
  pub fn set_file_range(&self, tbl: &TableIdent, range_li: &[BranchFileRange]) {
    let mut map = self.file_range_map.borrow_mut();
    if map.get(tbl).is_none_or(|old| old != range_li) {
      map.insert(tbl.clone(), range_li.to_vec());
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// 借用当前 branch / term；格式化 key 期间持有 `Ref`，不再 clone branch。
  pub(crate) fn identity(&self) -> Ref<'_, FileIdentity> {
    self.identity.borrow()
  }
}

impl<I, O> CloudStore<I, O> {
  /// `<prefix>/<table>.<partition>`，与 C++ `ComposeKey` 的第一段一致。
  pub(crate) fn tbl_prefix(&self, tbl: &TableIdent) -> String {
    let prefix = self.prefix.trim_matches('/');
    if prefix.is_empty() {
      tbl.to_string()
    } else {
      format!("{prefix}/{tbl}")
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// 根对象 key（term 文件等非分区对象）：`<prefix>/<name>`，空 prefix 时是裸名字。
  ///
  /// 与 [`Self::tbl_prefix`] 同一规则，对齐 C++ `ComposeKeyFromRemote`：空组件直接
  /// 跳过、不加分隔符。此前 term key 用 `format!("{}/{}", ...)`，空 prefix 下会多出
  /// 前导 `/`，与同 bucket 里 data/manifest key 分裂，跨实现（C++ master）互相看不见
  /// 对方的 term 文件、fencing 静默失效。
  pub(crate) fn root_object_key(&self, name: &str) -> String {
    let prefix = self.prefix.trim_matches('/');
    if prefix.is_empty() {
      name.to_owned()
    } else {
      format!("{prefix}/{name}")
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// 列目录用：`<prefix>/<table>.<partition>/`。
  pub(crate) fn tbl_scan_prefix(&self, tbl: &TableIdent) -> String {
    format!("{}/", self.tbl_prefix(tbl))
  }
}

impl<I, O> CloudStore<I, O> {
  /// manifest 对象列举前缀：`<prefix>/<table>.<partition>/manifest_`。
  ///
  /// 同一 table 目录下 data / segment 对象可能上百万；manifest 选择与剪枝只
  /// 关心 manifest 名字，按该前缀列举避免扫整目录。
  pub(crate) fn manifest_scan_prefix(&self, tbl: &TableIdent) -> String {
    format!("{}manifest_", self.tbl_scan_prefix(tbl))
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn object_key(&self, tbl: &TableIdent, name: &str) -> String {
    format!("{}/{name}", self.tbl_prefix(tbl))
  }
}

impl<I, O> CloudStore<I, O> {
  /// 按 `file_range_li` 链解析 file id 的写时 `(branch, term)`；链为空或 file id
  /// 超出链尾（当前 term 新分配）时回退到当前 identity。
  pub(crate) fn ranged_name(&self, tbl: &TableIdent, file_id: FileId, segment: bool) -> String {
    let id = self.identity();
    let range_map = self.file_range_map.borrow();
    let (branch, term) = range_map
      .get(tbl)
      .and_then(|range_li| range_ident(range_li, segment, file_id))
      .unwrap_or((&id.branch, id.term));
    if segment {
      fkv_types::segment_file_name(file_id, branch, term)
    } else {
      fkv_types::data_file_name(file_id, branch, term)
    }
  }
}

impl<I, O> CloudStore<I, O> {
  /// data 对象（读 / 删）：`<prefix>/<table>/data_<file_id>_<branch>_<term>`。
  pub(crate) fn file_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    self.object_key(tbl, &self.ranged_name(tbl, file_id, false))
  }
}

impl<I, O> CloudStore<I, O> {
  /// data 对象（写）：只按当前 identity，绝不回退旧 term key。
  pub(crate) fn write_file_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    let id = self.identity();
    self.object_key(
      tbl,
      &fkv_types::data_file_name(file_id, &id.branch, id.term),
    )
  }
}

impl<I, O> CloudStore<I, O> {
  /// 当前 active manifest 对象：`<prefix>/<table>/manifest_<branch>_<term>`。
  pub(crate) fn manifest_key(&self, tbl: &TableIdent) -> String {
    let id = self.identity();
    self.object_key(tbl, &fkv_types::manifest_file_name(&id.branch, id.term))
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn manifest_file_key(&self, tbl: &TableIdent, name: &str) -> String {
    self.object_key(tbl, name)
  }
}

impl<I, O> CloudStore<I, O> {
  /// segment 对象（读 / 删）：`<prefix>/<table>/segment_<file_id>_<branch>_<term>`。
  pub(crate) fn segment_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    self.object_key(tbl, &self.ranged_name(tbl, file_id, true))
  }
}

impl<I, O> CloudStore<I, O> {
  /// segment 对象（写）：只按当前 identity。
  pub(crate) fn write_segment_key(&self, tbl: &TableIdent, file_id: FileId) -> String {
    let id = self.identity();
    self.object_key(
      tbl,
      &fkv_types::segment_file_name(file_id, &id.branch, id.term),
    )
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn segment_file_id(&self, segment_fp_id: FilePageId) -> FileId {
    segment_fp_id >> self.segments_per_file_shift
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn file_id_of(&self, file_page_id: FilePageId) -> FileId {
    file_page_id >> self.pages_per_file_shift
  }
}

impl<I, O> CloudStore<I, O> {
  pub(crate) fn page_size_of(&self, file_page_id: FilePageId, src_len: usize) -> u64 {
    let index = file_page_id & ((1u64 << self.pages_per_file_shift) - 1);
    (index + 1) * src_len as u64
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 从对象 key 解析分区名。key 形如 `<prefix>/<tbl>.<partition>/<name>`；
  /// 不在本 prefix 下、或第一段目录名不合规（如 term 文件）时返回 `None`。
  pub(crate) fn partition_of_key(&self, key: &str) -> Option<TableIdent> {
    let prefix = self.prefix.trim_matches('/');
    let rest = if prefix.is_empty() {
      key
    } else {
      key.strip_prefix(prefix)?.strip_prefix('/')?
    };
    let dir = rest.split_once('/')?.0;
    TableIdent::parse_dir_name(dir)
  }
}

impl<I, O: ObjectStore> CloudStore<I, O> {
  /// 设置当前 `(branch, term)`：与 [`PageIo::set_file_identity`] 同语义。
  ///
  /// 供预热后台线程在列举远端对象前对齐目标 shard 的 attach 身份——远端
  /// data / segment 对象名里带写时的 branch / term，身份不对会列举不到。
  pub fn set_identity(&self, branch: &str, term: u64)
  where
    I: ManifestIo + PageIo + SegmentIo + FileIo,
  {
    PageIo::set_file_identity(self, branch, term);
  }
}
