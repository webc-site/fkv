//! pooled-08 Runtime 端到端：`start_mem`（默认 Pooled）+ `attach_shard` 打开
//! 较早创建的 branch 后，兄弟分支的预留块不得抬高本分支的分配水位。
//!
//! issue 的复现序列（修复前 `feature get a` 正常，但 `put f` 返回
//! `OutOfSpace`，随后 `main get m = None`——失败批已把越界页 flush 到父表
//! 页 id 上）：
//!
//! ```text
//! main(term1): put a → create_branch(feature) → create_branch(other) → put m
//! feature(term2): get a → put f
//! main(term3): get m（修复前 None）→ get a
//! ```

use fkv_btree::BatchEntry;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

fn up(key: &[u8], value: &[u8], ts: u64) -> Vec<BatchEntry> {
  vec![BatchEntry::upsert(key.to_vec(), value.to_vec(), ts)]
}

#[test]
fn start_mem_attach_earlier_sibling_branch_keeps_parent_data() {
  let root = TempRoot::new("p08_runtime_sibling");
  pollster::block_on(async {
    let runtime = Runtime::start_mem(RuntimeConfig::new(root.path().to_path_buf())).unwrap();
    let tbl = TableIdent::new("p08rt", 1).unwrap();

    runtime.attach_shard(0, "main", 1).await.unwrap();
    runtime.put(&tbl, up(b"a", b"A", 1)).await.unwrap();
    runtime.create_branch(&tbl, "feature").await.unwrap();
    runtime.create_branch(&tbl, "other").await.unwrap();
    runtime.put(&tbl, up(b"m", b"MAIN", 2)).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec())
    );

    // 打开较早创建的 feature：limit 只在它自己的块内，写必须成功。
    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "feature", 2).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));
    runtime.put(&tbl, up(b"f", b"F", 3)).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"f").await.unwrap(), Some(b"F".to_vec()));

    // 回 main：父表数据不得被越界 flush 覆盖。
    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "main", 3).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec()),
      "attach 较早 branch + 写覆盖了父表数据"
    );
    assert_eq!(runtime.get(&tbl, b"a").await.unwrap(), Some(b"A".to_vec()));

    // feature 重开仍读到自己的写。
    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "feature", 4).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"f").await.unwrap(), Some(b"F".to_vec()));

    // 兄弟 other 自己可写。
    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "other", 5).await.unwrap();
    runtime.put(&tbl, up(b"o", b"O", 6)).await.unwrap();
    assert_eq!(runtime.get(&tbl, b"o").await.unwrap(), Some(b"O".to_vec()));

    runtime.detach_shard(0).await.unwrap();
    runtime.attach_shard(0, "main", 7).await.unwrap();
    assert_eq!(
      runtime.get(&tbl, b"m").await.unwrap(),
      Some(b"MAIN".to_vec())
    );
    runtime.shutdown().await.unwrap();
  });
}
