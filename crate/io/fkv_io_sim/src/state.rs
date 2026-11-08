//! 模拟后端的状态层：durable / volatile 两层存储、逻辑时钟与盘面快照。

use core::{cmp::Ordering, hash::Hash};

use fkv_error::Result;
use fkv_types::{FilePageId, TableIdent};
use gxhash::HashMap as GxHashMap;

use crate::op::{Fault, Op, OpKind, OpRecord};

pub(crate) type PageKey = (TableIdent, FilePageId);
pub(crate) type ManifestFileKey = (TableIdent, String);
pub(crate) type TermKey = (String, u32);

pub(crate) type PageMap = GxHashMap<PageKey, Vec<u8>>;
pub(crate) type PageOverlay = GxHashMap<PageKey, Option<Vec<u8>>>;
pub(crate) type ManifestMap = GxHashMap<TableIdent, Vec<u8>>;
pub(crate) type ManifestFileMap = GxHashMap<ManifestFileKey, Vec<u8>>;
pub(crate) type TermMap = GxHashMap<TermKey, u64>;

#[derive(Debug, Default)]
pub(crate) struct Config {
  pub(crate) delay_li: Vec<(OpKind, u64)>,
  pub(crate) fault_li: Vec<Fault>,
  pub(crate) reorder_seed: Option<u64>,
  pub(crate) capture_payload: bool,
}

/// 已 fsync 的一层状态。
#[derive(Debug, Default)]
pub(crate) struct Layer {
  pub(crate) page_map: PageMap,
  pub(crate) segment_map: PageMap,
  pub(crate) manifest_map: ManifestMap,
  pub(crate) manifest_file_map: ManifestFileMap,
  pub(crate) term_map: TermMap,
}

/// 未 fsync 的覆盖层：`None` 是删除墓碑。
#[derive(Debug, Default)]
pub(crate) struct Overlay {
  pub(crate) page_map: PageOverlay,
  pub(crate) segment_map: PageOverlay,
}

#[derive(Debug, Default)]
pub(crate) struct SimState {
  pub(crate) durable: Layer,
  pub(crate) volatile: Overlay,
  pub(crate) op_log_li: Vec<OpRecord>,
  pub(crate) op_count: GxHashMap<OpKind, u32>,
  pub(crate) clock: u64,
}

/// 盘面快照（排序去重后的逻辑视图），供「两次跑逐字节相等」断言。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
  pub page_li: Vec<(TableIdent, FilePageId, Vec<u8>)>,
  pub segment_li: Vec<(TableIdent, FilePageId, Vec<u8>)>,
  pub manifest_li: Vec<(TableIdent, Vec<u8>)>,
  pub manifest_file_li: Vec<(TableIdent, String, Vec<u8>)>,
  pub term_li: Vec<(String, u32, u64)>,
}

/// 记录一次逻辑操作；命中故障时返回错误且调用方不得改状态。
pub(crate) fn log_op(config: &Config, state: &mut SimState, op: Op) -> Result<()> {
  log_op_payload(config, state, op, None)
}

/// 带 payload 的记录入口；`capture_payload` 关闭时 payload 恒为 `None`。
pub(crate) fn log_op_payload(
  config: &Config,
  state: &mut SimState,
  op: Op,
  payload: Option<&[u8]>,
) -> Result<()> {
  state.clock += 1;
  let kind = op.kind();
  let count = state.op_count.entry(kind).or_insert(0);
  *count += 1;
  let at = *count;
  let delay = config
    .delay_li
    .iter()
    .find(|(k, _)| *k == kind)
    .map_or(0, |(_, delay)| *delay);
  state.op_log_li.push(OpRecord {
    op,
    clock: state.clock,
    delay,
    payload: payload
      .filter(|_| config.capture_payload)
      .map(<[u8]>::to_vec),
  });
  if let Some(fault) = config
    .fault_li
    .iter()
    .find(|fault| fault.op == kind && fault.at == at)
  {
    return Err(fault.err);
  }
  Ok(())
}

/// volatile 覆盖 durable 的读取。
pub(crate) fn overlay_get<K, V>(
  durable: &GxHashMap<K, V>,
  volatile: &GxHashMap<K, Option<V>>,
  key: &K,
) -> Option<V>
where
  K: Eq + Hash,
  V: Clone,
{
  match volatile.get(key) {
    Some(entry) => entry.clone(),
    None => durable.get(key).cloned(),
  }
}

pub(crate) fn tbl_cmp(a: &TableIdent, b: &TableIdent) -> Ordering {
  (a.tbl_name.as_ref(), a.partition_id).cmp(&(b.tbl_name.as_ref(), b.partition_id))
}

pub(crate) fn page_key_cmp(a: &PageKey, b: &PageKey) -> Ordering {
  tbl_cmp(&a.0, &b.0).then(a.1.cmp(&b.1))
}

pub(crate) fn sort_page_key_li(key_li: &mut [PageKey]) {
  key_li.sort_by(page_key_cmp);
}

/// 某 table 当前可见（未删除）的页键，排序去重。
pub(crate) fn live_page_key_li(
  durable: &Layer,
  volatile: &Overlay,
  tbl: &TableIdent,
) -> Vec<PageKey> {
  let mut keys: Vec<PageKey> = Vec::new();
  for key in durable.page_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.page_map, &volatile.page_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  for key in volatile.page_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.page_map, &volatile.page_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  sort_page_key_li(&mut keys);
  keys.dedup();
  keys
}

/// 某 table 在两层里出现过的页键（含墓碑），用于删除 / 覆盖时打墓碑。
pub(crate) fn existing_page_key_li(
  durable: &Layer,
  volatile: &Overlay,
  tbl: &TableIdent,
) -> Vec<PageKey> {
  let mut keys: Vec<PageKey> = durable
    .page_map
    .keys()
    .chain(volatile.page_map.keys())
    .filter(|key| &key.0 == tbl)
    .map(PageKey::clone)
    .collect();
  sort_page_key_li(&mut keys);
  keys.dedup();
  keys
}

pub(crate) fn live_segment_key_li(
  durable: &Layer,
  volatile: &Overlay,
  tbl: &TableIdent,
) -> Vec<PageKey> {
  let mut keys: Vec<PageKey> = Vec::new();
  for key in durable.segment_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.segment_map, &volatile.segment_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  for key in volatile.segment_map.keys() {
    if &key.0 == tbl && overlay_get(&durable.segment_map, &volatile.segment_map, key).is_some() {
      keys.push(key.clone());
    }
  }
  sort_page_key_li(&mut keys);
  keys.dedup();
  keys
}

pub(crate) fn snapshot_of(state: &SimState) -> Snapshot {
  let mut snapshot = Snapshot::default();

  let mut page_keys: Vec<PageKey> = Vec::new();
  page_keys.extend(state.durable.page_map.keys().cloned());
  page_keys.extend(state.volatile.page_map.keys().cloned());
  sort_page_key_li(&mut page_keys);
  page_keys.dedup();
  for key in page_keys {
    if let Some(bytes) = overlay_get(&state.durable.page_map, &state.volatile.page_map, &key) {
      snapshot.page_li.push((key.0, key.1, bytes));
    }
  }

  let mut segment_keys: Vec<PageKey> = Vec::new();
  segment_keys.extend(state.durable.segment_map.keys().cloned());
  segment_keys.extend(state.volatile.segment_map.keys().cloned());
  sort_page_key_li(&mut segment_keys);
  segment_keys.dedup();
  for key in segment_keys {
    if let Some(bytes) = overlay_get(
      &state.durable.segment_map,
      &state.volatile.segment_map,
      &key,
    ) {
      snapshot.segment_li.push((key.0, key.1, bytes));
    }
  }

  let mut manifest_keys: Vec<TableIdent> = state.durable.manifest_map.keys().cloned().collect();
  manifest_keys.sort_by(tbl_cmp);
  manifest_keys.dedup();
  for tbl in manifest_keys {
    if let Some(bytes) = state.durable.manifest_map.get(&tbl) {
      let bytes = bytes.clone();
      snapshot.manifest_li.push((tbl, bytes));
    }
  }

  let mut manifest_file_keys: Vec<ManifestFileKey> =
    state.durable.manifest_file_map.keys().cloned().collect();
  manifest_file_keys.sort_by(|a, b| tbl_cmp(&a.0, &b.0).then(a.1.cmp(&b.1)));
  manifest_file_keys.dedup();
  for key in manifest_file_keys {
    if let Some(bytes) = state.durable.manifest_file_map.get(&key) {
      let bytes = bytes.clone();
      snapshot.manifest_file_li.push((key.0, key.1, bytes));
    }
  }

  let mut term_keys: Vec<TermKey> = state.durable.term_map.keys().cloned().collect();
  term_keys.sort();
  for key in term_keys {
    let term = state.durable.term_map[&key];
    snapshot.term_li.push((key.0, key.1, term));
  }

  snapshot
}
