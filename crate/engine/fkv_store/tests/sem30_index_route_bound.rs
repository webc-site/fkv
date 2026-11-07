//! sem-30 回归：小页下索引 root 分裂后的写路由必须尊重**祖先**上界。
//!
//! 现场：`page_size=512`、value 100B、约 16k 个键顺序单键写入时，点读漏掉尾部
//! 1281 个键（restart=8），而 scan 仍能返回全部键。根因在写路径
//! `RouteCursor::page_for`：判断「key 是否仍在当前子树」时只看了直接父节点的
//! 下一个兄弟，漏掉「父节点本身还有祖先兄弟」的情况。root 分裂后，左子树
//! 的末个子页越过 root 分隔键仍被当作在范围内，超界 key 被写进该子树的最后一
//! 个 data 页；新页 / 新分隔键挂到错误父页，点读按分隔键路由到不含该 key 的
//! 子树，整段静默漏读（scan 沿 data 页 next 链走，故仍可见）。
//!
//! 触发需要索引树至少 4 层（root + 非叶 index + 叶 index + data）：512B 页在
//! 约 16k 键处 leaf index 页数超过 root 容量，root 分裂后即满足条件；1024/4096
//! 页在同规模下只有 3 层，观察不到。因此用例固定 512B 页。

use fkv_btree::BatchEntry;
use fkv_io_mem::MemStore;
use fkv_store::{AllocMode, Store};
use fkv_types::TableIdent;

const VALUE_LEN: usize = 100;
const PAGES_PER_FILE_SHIFT: u8 = 2;

/// 确定性打乱（LCG），避免测试依赖随机数实现。
fn shuffled_order(total: u64) -> Vec<u64> {
  let mut order: Vec<u64> = (0..total).collect();
  let mut x = 12345u64;
  for i in (1..order.len()).rev() {
    x = x
      .wrapping_mul(6364136223846793005)
      .wrapping_add(1442695040888963407);
    let j = (x >> 33) as usize % (i + 1);
    order.swap(i, j);
  }
  order
}

/// 写入后逐键点读，并与 scan 结果对拍：键集合、顺序、value 全部一致。
async fn check_point_get_matches_scan(
  tag: &str,
  restart: u16,
  total: u64,
  shuffled: bool,
  batch: usize,
) {
  let tbl = TableIdent::new(tag, 1).unwrap();
  let mut store = Store::new(
    MemStore::with_pages_per_file(512, PAGES_PER_FILE_SHIFT),
    512,
    restart,
  )
  .with_pages_per_file_shift(PAGES_PER_FILE_SHIFT)
  .with_alloc_mode(AllocMode::Pooled);
  let value = vec![0x5a; VALUE_LEN];
  let order = if shuffled {
    shuffled_order(total)
  } else {
    (0..total).collect()
  };
  for chunk in order.chunks(batch) {
    let entry_li: Vec<_> = chunk
      .iter()
      .map(|&i| BatchEntry::upsert(format!("m{i:05}").into_bytes(), value.clone(), 1 + i))
      .collect();
    store.put(&tbl, &entry_li).await.unwrap();
  }

  // scan 必须覆盖全部键且严格升序：页链顺序错乱也是同一路由缺陷的症状。
  let rows = store.scan(&tbl, b"", b"", 0).await.unwrap();
  assert_eq!(rows.len() as u64, total, "{tag}: scan 行数");
  for (idx, (key, _)) in rows.iter().enumerate() {
    let expected = format!("m{idx:05}");
    assert_eq!(
      key.as_slice(),
      expected.as_bytes(),
      "{tag}: scan 第 {idx} 项顺序错误"
    );
  }

  // 点读对拍：每个 scan 可见的键都必须 get 得到，且 value 一致。
  for (idx, (key, expected)) in rows.iter().enumerate() {
    let got = store.get(&tbl, key).await.unwrap();
    assert_eq!(
      got.as_ref(),
      Some(expected),
      "{tag}: 点读漏键 idx={idx} key={}",
      String::from_utf8_lossy(key)
    );
  }
}

/// 现场用例：顺序单键（batch=1）写入，修复前漏 1281/16006。
#[test]
fn seq_single_put_no_point_get_miss() {
  pollster::block_on(check_point_get_matches_scan(
    "sem30_seq_r8",
    8,
    16006,
    false,
    1,
  ));
}

/// 乱序单键写入（restart=16）：修复前漏 2790/16006。
#[test]
fn random_single_put_no_point_get_miss_restart16() {
  pollster::block_on(check_point_get_matches_scan(
    "sem30_random_r16",
    16,
    16006,
    true,
    1,
  ));
}

/// 乱序批量写入（100/批）：修复前漏 3864/16006（issue 原报告只测了顺序批量）。
#[test]
fn random_batch_put_no_point_get_miss() {
  pollster::block_on(check_point_get_matches_scan(
    "sem30_random_b100",
    8,
    16006,
    true,
    100,
  ));
}
