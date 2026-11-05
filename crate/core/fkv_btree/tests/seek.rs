//! `decompress_value` 零拷贝与 `IndexNode::seek` restart 二分。

use std::borrow::Cow;

use fkv_btree::{IndexNode, decompress_value};
use fkv_page::{CompressionType, IndexPageBuilder};

#[test]
fn decompress_none_borrows() {
  let value = b"plain".as_slice();
  let out = decompress_value(value, CompressionType::None, None).unwrap();
  assert!(matches!(out, Cow::Borrowed(_)));
  assert_eq!(out.as_ptr(), value.as_ptr());
}

#[test]
fn decompress_dict_without_dict_fails() {
  assert!(decompress_value(b"x", CompressionType::Dictionary, None).is_err());
}

/// 宽 index page、多种 restart_interval 下与分段语义一致。
#[test]
fn index_node_seek_wide_page() {
  const KEY_CNT: u32 = 200;
  const LEFTMOST: u32 = 7;
  for interval in [1u16, 2, 16] {
    let mut builder = IndexPageBuilder::new(4096, interval, false).unwrap();
    builder.add(b"", LEFTMOST).unwrap();
    // key = 偶数的 4 字节大端，child = key + 1000
    for n in 0..KEY_CNT {
      assert!(
        builder
          .add(&(n * 2 + 2).to_be_bytes(), n * 2 + 1002)
          .unwrap()
      );
    }
    let page = builder.finish().unwrap();
    let node = IndexNode::new(0, &page).unwrap();
    assert_eq!(node.seek(b"").unwrap(), LEFTMOST);
    assert_eq!(node.seek(&1u32.to_be_bytes()).unwrap(), LEFTMOST);
    for probe in 2..KEY_CNT * 2 + 4 {
      let floor = (probe.min(KEY_CNT * 2)) & !1;
      assert_eq!(
        node.seek(&probe.to_be_bytes()).unwrap(),
        floor + 1000,
        "probe {probe} interval {interval}"
      );
    }
  }
}
