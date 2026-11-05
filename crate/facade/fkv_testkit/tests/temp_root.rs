use std::{
  fs::{create_dir_all, remove_dir_all, write},
  path::Path,
};

use fkv_testkit::TempRoot;

#[test]
fn temp_root_unique_and_cleaned_on_drop() {
  let first = TempRoot::new("fkv_test");
  let second = TempRoot::new("fkv_test");
  assert!(first.path().is_dir());
  assert_ne!(first.path(), second.path());

  let nested = first.join("a/b");
  create_dir_all(&nested).unwrap();
  write(nested.join("page"), b"page").unwrap();
  assert!(nested.join("page").is_file());

  let first_path = first.path().to_path_buf();
  drop(first);
  assert!(!Path::new(&first_path).exists());
  // 另一个根不受影响。
  assert!(second.path().is_dir());
}

#[test]
fn temp_root_sanitizes_tag_and_into_path_keeps_dir() {
  let root = TempRoot::new("weird/../tag:1");
  let name = root
    .path()
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap();
  assert!(name.starts_with("fkv_weird____tag_1_"));

  let kept = root.into_path();
  assert!(kept.is_dir());
  remove_dir_all(&kept).unwrap();
}

#[test]
fn temp_root_try_new_creates_dir() {
  let root = TempRoot::try_new("try").unwrap();
  assert!(root.path().is_dir());
}
