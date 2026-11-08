//! 控制面请求判定：无 `tbl` 的 `ListStandbyPartition` 必须与 attach / detach /
//! shutdown 同列，不能按调用方 `tbl` 路由到任意 shard。

use fkv_api::Request;
use fkv_types::TableIdent;

fn tbl() -> TableIdent {
  TableIdent::new("demo", 1).unwrap()
}

/// `ListStandbyPartition` 没有 `tbl`，且固定由 shard 0 的 standby service 应答：
/// 按调用方 `tbl` 路由会打到错 shard。
#[test]
fn list_standby_partition_is_control_plane() {
  let req = Request::ListStandbyPartition;
  assert!(req.table().is_none(), "该请求不带分区");
  assert!(req.is_control_plane(), "无 tbl 的列举必须按控制面拒绝");
}

/// 数据面请求（含带 `tbl` 的 Cloud 列举）不受控制面判定影响。
#[test]
fn data_plane_requests_are_not_control_plane() {
  let tbl = tbl();
  assert!(
    !Request::Get {
      tbl: tbl.clone(),
      key: b"k".to_vec()
    }
    .is_control_plane()
  );
  assert!(
    !Request::ListObject {
      tbl,
      prefix: String::new(),
      token: None,
      max_keys: 0,
      recursive: false,
    }
    .is_control_plane()
  );
  assert!(
    !Request::Prewarm {
      tbl: TableIdent::new("demo", 2).unwrap()
    }
    .is_control_plane()
  );
}
