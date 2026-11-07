//! 集成测试共用 helper：manifest log / snapshot 段的解析与盘面页读取。
//!
//! `tests/*.rs` 各自是独立 test target，均以 `mod common;` 引入；每个 target
//! 只引用所需函数，未引用者不报 dead_code。
#![allow(dead_code)]

use fkv_codec::{get_varint32, get_varint64};
use fkv_io::PageIo;
use fkv_io_mem::MemStore;
use fkv_manifest::payload;
use fkv_page::{DataPageIter, OverflowPage, PageType, decode_pointers, page_type};
use fkv_store::Store;
use fkv_types::TableIdent;

/// log record 的 mapping 段：`mapping_len(4B) | (page_id varint32, value varint64)*`。
pub fn log_mapping(record: &[u8]) -> &[u8] {
  let body = payload(record).expect("log payload");
  let len = u32::from_le_bytes(body[..4].try_into().expect("mapping_len")) as usize;
  body.get(4..4 + len).expect("mapping 段越界")
}

/// 解析 log mapping 段为 `(page_id, value)` 序列，必须恰好耗尽。
pub fn log_mapping_entry_li(record: &[u8]) -> Vec<(u32, u64)> {
  let section = log_mapping(record);
  let mut pos = 0usize;
  let mut entry_li = Vec::new();
  while pos < section.len() {
    let (page_id, read) = get_varint32(&section[pos..]).expect("page id");
    pos += read;
    let (value, read) = get_varint64(&section[pos..]).expect("mapping value");
    pos += read;
    entry_li.push((page_id, value));
  }
  entry_li
}

/// log record 的 segment 段：跳过 data mapping 段与 branch metadata 段后，
/// 读取 `segment_len(4B) | (page_id varint32, value varint64)*`。
pub fn log_segment(record: &[u8]) -> &[u8] {
  let body = payload(record).expect("log payload");
  let mapping_len = u32::from_le_bytes(body[..4].try_into().expect("mapping_len")) as usize;
  let branch_pos = 4 + mapping_len;
  let branch_len = u32::from_le_bytes(
    body[branch_pos..branch_pos + 4]
      .try_into()
      .expect("branch_len"),
  ) as usize;
  let segment_pos = branch_pos + 4 + branch_len;
  let segment_len = u32::from_le_bytes(
    body[segment_pos..segment_pos + 4]
      .try_into()
      .expect("segment_len"),
  ) as usize;
  body
    .get(segment_pos + 4..segment_pos + 4 + segment_len)
    .expect("segment 段越界")
}

/// 解析 log segment 段为 `(page_id, value)` 序列，必须恰好耗尽。
pub fn log_segment_entry_li(record: &[u8]) -> Vec<(u32, u64)> {
  let section = log_segment(record);
  let mut pos = 0usize;
  let mut entry_li = Vec::new();
  while pos < section.len() {
    let (page_id, read) = get_varint32(&section[pos..]).expect("page id");
    pos += read;
    let (value, read) = get_varint64(&section[pos..]).expect("segment value");
    pos += read;
    entry_li.push((page_id, value));
  }
  entry_li
}

/// snapshot 的 mapping 段：`max_fp_id | dict_len | dict | mapping_len(4B) | values`。
pub fn snapshot_mapping(record: &[u8]) -> &[u8] {
  let body = payload(record).expect("snapshot payload");
  let (_, read) = get_varint64(body).expect("max_fp_id");
  let mut pos = read;
  let (dict_len, read) = get_varint32(&body[pos..]).expect("dict_len");
  pos += read + dict_len as usize;
  let len = u32::from_le_bytes(body[pos..pos + 4].try_into().expect("mapping_len")) as usize;
  &body[pos + 4..pos + 4 + len]
}

/// 从盘面读一页（Mem 后端直接读已落盘页）。
pub async fn page_bytes(store: &Store<MemStore>, tbl: &TableIdent, page_id: u32) -> Vec<u8> {
  let file_page_id = store.mapper(tbl).unwrap().file_page_id(page_id).unwrap();
  let mut page = vec![0u8; store.page_size()];
  store
    .page_io()
    .read_page(tbl, file_page_id, &mut page)
    .await
    .unwrap();
  page
}

/// 从 data page 的 leaf entry 读出某 key 的完整 overflow 链：首批指针 + 各批
/// 尾页的下一批指针，按盘面链序展开（前向 = 分配顺序）。
pub async fn read_overflow_chain(
  store: &Store<MemStore>,
  tbl: &TableIdent,
  data_page: &[u8],
  key: &[u8],
) -> Vec<u32> {
  let mut iter = DataPageIter::new(data_page).unwrap();
  let mut chain = Vec::new();
  while iter.advance().unwrap() {
    if iter.key() == key {
      assert!(iter.is_overflow(), "12 KiB 值必须是 overflow entry");
      chain = decode_pointers(iter.value()).unwrap();
      break;
    }
  }
  assert!(
    !chain.is_empty(),
    "data page 必须有该 key 的 overflow entry"
  );
  let mut full = Vec::new();
  loop {
    full.extend_from_slice(&chain);
    let end_page = *chain.last().unwrap();
    let page = page_bytes(store, tbl, end_page).await;
    let next = OverflowPage::new(&page).unwrap().pointer_li().unwrap();
    if next.is_empty() {
      return full;
    }
    chain = next;
  }
}

/// 找表里唯一的 data 页（Append 分配下页类型可逐页判定）。
pub async fn find_data_page(store: &Store<MemStore>, tbl: &TableIdent, page_num: u32) -> u32 {
  let mut data_page_id = None;
  for page_id in 0..page_num {
    let page = page_bytes(store, tbl, page_id).await;
    if page_type(&page).unwrap() == PageType::Data {
      assert!(data_page_id.is_none(), "应只有一张 data 页");
      data_page_id = Some(page_id);
    }
  }
  data_page_id.expect("data page")
}
