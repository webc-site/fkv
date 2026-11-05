//! 对齐 C++ `tests/branch_filename_parsing.cpp` 的分支文件名解析矩阵：
//! 归一 / 校验 / 生成 / 解析 / roundtrip / 检测。
//!
//! 差异说明：C++ 在 `BranchDataFileName` 等生成函数内归一；Rust 把归一放在
//! API 边界（store / shard 先 `normalize_branch_name` 再生成），语义一致。
//! C++ 解析失败返回 false / 0；Rust 对应 `None`。

use fkv_types::{
  FILE_NAME_DATA, FILE_NAME_MANIFEST, FILE_NAME_SEGMENT, archive_file_name, current_term_file_name,
  data_file_name, is_expired, is_valid_branch_name, manifest_file_name, normalize_branch_name,
  parse_data_file_name, parse_manifest_file_name, segment_file_name,
};

// ---- NormalizeBranchName ----

#[test]
fn normalize_valid_names() {
  assert_eq!(normalize_branch_name("main").as_deref(), Some("main"));
  assert_eq!(normalize_branch_name("feature").as_deref(), Some("feature"));
  assert_eq!(normalize_branch_name("dev").as_deref(), Some("dev"));
  assert_eq!(
    normalize_branch_name("feature123").as_deref(),
    Some("feature123")
  );
  assert_eq!(normalize_branch_name("v2").as_deref(), Some("v2"));
  assert_eq!(normalize_branch_name("123").as_deref(), Some("123"));
  assert_eq!(
    normalize_branch_name("feature-branch").as_deref(),
    Some("feature-branch")
  );
  assert_eq!(
    normalize_branch_name("my-feature-123").as_deref(),
    Some("my-feature-123")
  );
  assert_eq!(
    normalize_branch_name("feat-123-dev").as_deref(),
    Some("feat-123-dev")
  );
}

#[test]
fn normalize_case_folding() {
  assert_eq!(normalize_branch_name("MAIN").as_deref(), Some("main"));
  assert_eq!(normalize_branch_name("FEATURE").as_deref(), Some("feature"));
  assert_eq!(normalize_branch_name("DEV").as_deref(), Some("dev"));
  assert_eq!(normalize_branch_name("Feature").as_deref(), Some("feature"));
  assert_eq!(
    normalize_branch_name("MyFeature").as_deref(),
    Some("myfeature")
  );
  assert_eq!(normalize_branch_name("FeAtUrE").as_deref(), Some("feature"));
  assert_eq!(
    normalize_branch_name("Feature-Branch").as_deref(),
    Some("feature-branch")
  );
  assert_eq!(
    normalize_branch_name("Feature-123").as_deref(),
    Some("feature-123")
  );
}

#[test]
fn normalize_rejects_invalid_chars() {
  // C++ 以空串表示失败；Rust 对应 `None`。
  assert_eq!(normalize_branch_name(""), None);
  assert_eq!(normalize_branch_name("feature branch"), None); // 空格
  assert_eq!(normalize_branch_name("feature.branch"), None); // 点
  assert_eq!(normalize_branch_name("feature@branch"), None);
  assert_eq!(normalize_branch_name("feature#branch"), None);
  assert_eq!(normalize_branch_name("feature$branch"), None);
  assert_eq!(normalize_branch_name("feature/branch"), None);
  assert_eq!(normalize_branch_name("feature\\branch"), None);
  assert_eq!(normalize_branch_name("feature:branch"), None);
  // 下划线非法：保留作 FileNameSeparator。
  assert_eq!(normalize_branch_name("feature_branch"), None);
  assert_eq!(normalize_branch_name("my_feature"), None);
}

#[test]
fn normalize_edge_cases() {
  assert_eq!(normalize_branch_name("a").as_deref(), Some("a"));
  assert_eq!(normalize_branch_name("A").as_deref(), Some("a"));
  assert_eq!(normalize_branch_name("1").as_deref(), Some("1"));
  assert_eq!(normalize_branch_name("-").as_deref(), Some("-"));
  assert_eq!(normalize_branch_name("_"), None);
  let long_name = "a".repeat(100);
  assert_eq!(
    normalize_branch_name(&long_name).as_deref(),
    Some(long_name.as_str())
  );
  assert_eq!(normalize_branch_name("Main").as_deref(), Some("main"));
  assert_eq!(normalize_branch_name("MAIN").as_deref(), Some("main"));
}

#[test]
fn is_valid_branch_name_wrapper() {
  assert!(is_valid_branch_name("main"));
  assert!(is_valid_branch_name("feature"));
  assert!(is_valid_branch_name("Feature123"));
  assert!(is_valid_branch_name("my-feature"));
  assert!(!is_valid_branch_name(""));
  assert!(!is_valid_branch_name("feature branch"));
  assert!(!is_valid_branch_name("feature.branch"));
  assert!(!is_valid_branch_name("feature@123"));
  assert!(!is_valid_branch_name("my_feature"));
}

// ---- 文件名生成 ----

#[test]
fn data_file_name_format() {
  assert_eq!(data_file_name(123, "main", 5), "data_123_main_5");
  assert_eq!(data_file_name(456, "feature", 10), "data_456_feature_10");
  assert_eq!(data_file_name(0, "main", 0), "data_0_main_0");
  assert_eq!(data_file_name(0, "feature", 1), "data_0_feature_1");
  assert_eq!(
    data_file_name(999_999, "main", 123_456),
    "data_999999_main_123456"
  );
  assert_eq!(data_file_name(10, "dev", 1), "data_10_dev_1");
  assert_eq!(
    data_file_name(10, "feature-123", 1),
    "data_10_feature-123_1"
  );
  assert_eq!(data_file_name(10, "hotfix-456", 1), "data_10_hotfix-456_1");
  // segment 与 data 共用 `<prefix>_<file_id>_<branch>_<term>` 布局。
  assert_eq!(segment_file_name(7, "main", 2), "segment_7_main_2");
}

#[test]
fn manifest_file_name_format() {
  assert_eq!(manifest_file_name("main", 5), "manifest_main_5");
  assert_eq!(manifest_file_name("feature", 10), "manifest_feature_10");
  assert_eq!(manifest_file_name("main", 0), "manifest_main_0");
  assert_eq!(
    manifest_file_name("main", 123_456_789),
    "manifest_main_123456789"
  );
  assert_eq!(manifest_file_name("dev", 1), "manifest_dev_1");
  assert_eq!(
    manifest_file_name("feature-123", 2),
    "manifest_feature-123_2"
  );
}

#[test]
fn archive_file_name_format() {
  assert_eq!(
    archive_file_name("main", 5, "123456"),
    "manifest_main_5_123456"
  );
  assert_eq!(
    archive_file_name("feature", 10, "789012"),
    "manifest_feature_10_789012"
  );
  assert_eq!(archive_file_name("main", 0, "0"), "manifest_main_0_0");
  assert_eq!(
    archive_file_name("main", 999, "1234567890123"),
    "manifest_main_999_1234567890123"
  );
}

#[test]
fn current_term_file_name_format() {
  assert_eq!(current_term_file_name("main", 0), "CURRENT_TERM_main_0");
  assert_eq!(
    current_term_file_name("feature", 3),
    "CURRENT_TERM_feature_3"
  );
  assert_eq!(current_term_file_name("dev", 42), "CURRENT_TERM_dev_42");
  assert_eq!(
    current_term_file_name("feature-123", 0),
    "CURRENT_TERM_feature-123_0"
  );
}

// ---- ParseDataFileSuffix（Rust 收全名，含 data_/segment_ 前缀）----

#[test]
fn parse_data_file_name_branch_format() {
  assert_eq!(
    parse_data_file_name("data_123_main_5"),
    Some((123, "main", 5))
  );
  assert_eq!(
    parse_data_file_name("data_456_feature_10"),
    Some((456, "feature", 10))
  );
  assert_eq!(parse_data_file_name("data_0_main_0"), Some((0, "main", 0)));
  assert_eq!(
    parse_data_file_name("data_10_feature-123_5"),
    Some((10, "feature-123", 5))
  );
  assert_eq!(
    parse_data_file_name("segment_10_feature-123_5"),
    Some((10, "feature-123", 5))
  );
}

#[test]
fn parse_data_file_name_keeps_case_as_is() {
  // 归一发生在生成时；解析按原样提取。
  assert_eq!(
    parse_data_file_name("data_456_Feature_10"),
    Some((456, "Feature", 10))
  );
}

#[test]
fn parse_data_file_name_rejects_old_format() {
  // 旧格式 file_id_term（无 branch）。
  assert_eq!(parse_data_file_name("data_123_5"), None);
  assert_eq!(parse_data_file_name("data_0_1"), None);
  // 更旧格式：仅 file_id。
  assert_eq!(parse_data_file_name("data_123"), None);
}

#[test]
fn parse_data_file_name_rejects_invalid() {
  assert_eq!(parse_data_file_name(""), None);
  assert_eq!(parse_data_file_name("abc_main_5"), None); // 非 "data_" 前缀
  assert_eq!(parse_data_file_name("data_abc_main_5"), None);
  assert_eq!(parse_data_file_name("data_123_main_abc"), None);
  assert_eq!(parse_data_file_name("data_123_main.branch_5"), None);
  assert_eq!(parse_data_file_name("data_123_main branch_5"), None);
  assert_eq!(parse_data_file_name("data_123_main_"), None);
  assert_eq!(parse_data_file_name("data__5"), None);
  assert_eq!(parse_data_file_name(FILE_NAME_SEGMENT), None);
  assert_eq!(parse_data_file_name(FILE_NAME_DATA), None);
}

// ---- ParseManifestFileSuffix（Rust 收全名，含 manifest_ 前缀）----

#[test]
fn parse_manifest_file_name_without_tag() {
  assert_eq!(
    parse_manifest_file_name("manifest_main_5"),
    Some(("main", 5, None))
  );
  assert_eq!(
    parse_manifest_file_name("manifest_feature_10"),
    Some(("feature", 10, None))
  );
  assert_eq!(
    parse_manifest_file_name("manifest_main_0"),
    Some(("main", 0, None))
  );
}

#[test]
fn parse_manifest_file_name_with_tag() {
  assert_eq!(
    parse_manifest_file_name("manifest_main_5_123456"),
    Some(("main", 5, Some("123456")))
  );
  assert_eq!(
    parse_manifest_file_name("manifest_feature_10_789012"),
    Some(("feature", 10, Some("789012")))
  );
  // tag 是任意串：0 与非数字都合法。
  assert_eq!(
    parse_manifest_file_name("manifest_main_5_0"),
    Some(("main", 5, Some("0")))
  );
  assert_eq!(
    parse_manifest_file_name("manifest_main_5_abc"),
    Some(("main", 5, Some("abc")))
  );
}

#[test]
fn parse_manifest_file_name_rejects_tmp_middle_state() {
  // 整写未 rename 的中间态不是可见 manifest：列举若解析成功，同一归档周期
  // 会同时看到 tmp 与正式名，(tag, term) 去重后仍多一条。
  assert_eq!(parse_manifest_file_name("manifest_main_5_123456.tmp"), None);
  assert_eq!(parse_manifest_file_name("manifest_main_5.tmp"), None);
}

#[test]
fn parse_manifest_file_name_keeps_case_as_is() {
  assert_eq!(
    parse_manifest_file_name("manifest_Feature_10_123"),
    Some(("Feature", 10, Some("123")))
  );
}

#[test]
fn parse_manifest_file_name_rejects_legacy() {
  // 旧格式：纯 term（branch 段为纯数字）必须拒绝，否则会被当成 branch 名。
  assert_eq!(parse_manifest_file_name("manifest_5"), None);
  assert_eq!(parse_manifest_file_name("manifest_0"), None);
  // 旧 archive 格式：term_timestamp（无 branch）。
  assert_eq!(parse_manifest_file_name("manifest_5_123456"), None);
}

#[test]
fn parse_manifest_file_name_rejects_invalid() {
  assert_eq!(parse_manifest_file_name(""), None);
  assert_eq!(parse_manifest_file_name(FILE_NAME_MANIFEST), None);
  assert_eq!(parse_manifest_file_name("manifest_main_abc"), None);
  assert_eq!(parse_manifest_file_name("manifest_main.branch_5"), None);
  assert_eq!(parse_manifest_file_name("manifest_main_"), None);
  assert_eq!(parse_manifest_file_name("manifest__5"), None);
}

// ---- 检测 helper（C++ IsBranchManifest / IsBranchArchive /
// IsBranchDataFile / ManifestTermFromFilename，Rust 以解析结果推导）----

#[test]
fn manifest_vs_archive_detection() {
  // manifest（无 tag）。
  for (name, branch, term) in [
    ("manifest_main_5", "main", 5),
    ("manifest_feature_10", "feature", 10),
  ] {
    let got = parse_manifest_file_name(name).unwrap();
    assert_eq!(got, (branch, term, None), "{name}");
  }
  // archive（有 tag）。
  for name in ["manifest_main_5_123456", "manifest_feature_10_789012"] {
    let (_, _, tag) = parse_manifest_file_name(name).unwrap();
    assert!(tag.is_some(), "{name}");
  }
  // 非 manifest 一律 None。
  assert_eq!(parse_manifest_file_name("data_123_main_5"), None);
  assert_eq!(parse_manifest_file_name("CURRENT_TERM.main"), None);
  assert_eq!(parse_manifest_file_name("invalid"), None);
}

#[test]
fn data_file_detection() {
  assert!(parse_data_file_name("data_123_main_5").is_some());
  assert!(parse_data_file_name("data_456_feature_10").is_some());
  assert!(parse_data_file_name("data_0_main_0").is_some());
  // 非 data 文件与旧格式。
  assert_eq!(parse_data_file_name("manifest_main_5"), None);
  assert_eq!(parse_data_file_name("manifest_main_5_123456"), None);
  assert_eq!(parse_data_file_name("CURRENT_TERM.main"), None);
  assert_eq!(parse_data_file_name("invalid"), None);
  assert_eq!(parse_data_file_name("data_123_5"), None);
  assert_eq!(parse_data_file_name("data_123"), None);
}

#[test]
fn manifest_term_from_filename() {
  assert_eq!(parse_manifest_file_name("manifest_main_5").unwrap().1, 5);
  assert_eq!(
    parse_manifest_file_name("manifest_feature_10").unwrap().1,
    10
  );
  assert_eq!(parse_manifest_file_name("manifest_main_0").unwrap().1, 0);
  // archive 也提取 term。
  assert_eq!(
    parse_manifest_file_name("manifest_main_5_123456")
      .unwrap()
      .1,
    5
  );
  // 旧格式 / 非法 → None（C++ 返回 0）。
  assert_eq!(parse_manifest_file_name("manifest_5"), None);
  assert_eq!(parse_manifest_file_name("invalid"), None);
  assert_eq!(parse_manifest_file_name(""), None);
}

// ---- Roundtrip ----

#[test]
fn roundtrip_data_files() {
  let name = data_file_name(123, "main", 5);
  assert_eq!(parse_data_file_name(&name), Some((123, "main", 5)));

  let name = data_file_name(999, "feature-123", 456);
  assert_eq!(parse_data_file_name(&name), Some((999, "feature-123", 456)));

  // 生成前先归一（Rust 在 API 边界做，C++ 在生成函数内做），解析得到小写。
  let normalized = normalize_branch_name("Feature").unwrap();
  let name = data_file_name(10, &normalized, 1);
  assert_eq!(parse_data_file_name(&name), Some((10, "feature", 1)));
}

#[test]
fn roundtrip_manifest_and_archive() {
  let name = manifest_file_name("main", 5);
  assert_eq!(parse_manifest_file_name(&name), Some(("main", 5, None)));
  let name = manifest_file_name("feature", 10);
  assert_eq!(parse_manifest_file_name(&name), Some(("feature", 10, None)));

  let name = archive_file_name("main", 5, "123456");
  assert_eq!(
    parse_manifest_file_name(&name),
    Some(("main", 5, Some("123456")))
  );
  let name = archive_file_name("feature", 10, "789012");
  assert_eq!(
    parse_manifest_file_name(&name),
    Some(("feature", 10, Some("789012")))
  );
}

#[test]
fn roundtrip_current_term() {
  assert_eq!(current_term_file_name("main", 0), "CURRENT_TERM_main_0");
  assert_eq!(
    current_term_file_name("feature", 7),
    "CURRENT_TERM_feature_7"
  );
  assert_eq!(
    current_term_file_name("feature-123", 0),
    "CURRENT_TERM_feature-123_0"
  );
}

// ---- 相邻行为锚定（C++ types 层同域语义）----

#[test]
fn ttl_expiry_boundary() {
  assert!(!is_expired(0, 1000)); // 0 = 永不过期
  assert!(!is_expired(1000, 999));
  assert!(is_expired(1000, 1000)); // expire <= now 视为过期
  assert!(is_expired(1000, 1001));
}

// ---- BranchFileMapping（C++ branch_filename_parsing.cpp 尾段映射矩阵）----

/// 对齐 C++ `BranchFileRange` 排序 + `FindBranchRange` bisect /
/// `FileIdInBranch`：自然序只按 `max_file_id`，与字段声明顺序无关。
#[test]
fn branch_file_range_sort_and_lookup() {
  use fkv_types::{BranchFileRange, FileId};

  let range = |branch: &str, term: u64, max_file_id: FileId| BranchFileRange {
    branch_name: branch.into(),
    term,
    max_file_id,
    max_segment_file_id: 0,
  };

  // 乱序构造：main(100) / feature(50) / hotfix(200)。
  let mut range_li = [
    range("main", 5, 100),
    range("feature", 3, 50),
    range("hotfix", 1, 200),
  ];
  range_li.sort_unstable_by_key(|r| r.max_file_id);
  assert_eq!(range_li[0].branch_name.as_ref(), "feature");
  assert_eq!(range_li[0].max_file_id, 50);
  assert_eq!(range_li[1].branch_name.as_ref(), "main");
  assert_eq!(range_li[2].branch_name.as_ref(), "hotfix");
  assert_eq!(range_li[2].max_file_id, 200);

  // FindBranchRange：file_id 落在哪个 range 的 (prev_max, max] 区间。
  let find = |file_id: FileId| {
    let idx = range_li.partition_point(|r| r.max_file_id < file_id);
    range_li.get(idx)
  };
  assert_eq!(find(25).map(|r| r.branch_name.as_ref()), Some("feature"));
  assert_eq!(find(75).map(|r| r.branch_name.as_ref()), Some("main"));
  assert_eq!(find(150).map(|r| r.branch_name.as_ref()), Some("hotfix"));
  // 超出上界（C++ 返回 end()）。
  assert_eq!(find(300), None);

  // FileIdInBranch。
  let in_branch =
    |file_id: FileId, branch: &str| find(file_id).is_some_and(|r| r.branch_name.as_ref() == branch);
  assert!(in_branch(25, "feature"));
  assert!(!in_branch(25, "main"));
  assert!(in_branch(75, "main"));
  assert!(!in_branch(75, "feature"));
  assert!(in_branch(150, "hotfix"));
  assert!(!in_branch(300, "hotfix"));
}

/// 空映射：查找返回 None。
#[test]
fn branch_file_range_empty_lookup() {
  use fkv_types::BranchFileRange;

  let range_li: Vec<BranchFileRange> = Vec::new();
  let idx = range_li.partition_point(|r| r.max_file_id < 50);
  assert!(range_li.get(idx).is_none());
}
