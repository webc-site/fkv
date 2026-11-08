//! 只读分发：`dispatch_read` 与串行路径 / 读任务共用的大值读执行体。

use fkv_api::{
  ArchiveTagInfo, FloorReply, LargeValueReply, ListPage, ObjectMeta, Request, Response, ScanReply,
};
use fkv_error::{KvError, Result};
use fkv_io::FixedBuffer;
use fkv_store::{PageOption, ScanOption};
use fkv_types::TableIdent;

use crate::partition::ShardStore;

/// 大值读执行体：优先读入 fixed buffer，后端报 `InvalidArgs` 时回落普通
/// 大值读。串行重试路径与读任务共用（io-05）。
pub(crate) async fn get_large_body(
  store: &ShardStore,
  tbl: &TableIdent,
  key: &[u8],
  buf: Option<&mut FixedBuffer>,
) -> Result<Option<LargeValueReply>> {
  let plain = || async {
    store.get_large(tbl, key).await.map(|large| {
      large.map(|large| LargeValueReply {
        value: large.value,
        metadata: large.metadata,
      })
    })
  };
  let Some(buf) = buf else {
    return plain().await;
  };
  match store.get_large_into(tbl, key, buf).await {
    Ok(value) => Ok(value.map(|into| LargeValueReply {
      value: buf.as_slice()[..into.value_len].to_vec(),
      metadata: into.metadata,
    })),
    // 后端不支持 fixed 直读（如 MemStore）：回落到普通大值读。
    Err(KvError::InvalidArgs) => plain().await,
    Err(err) => Err(err),
  }
}

/// `fkv_object::ObjectMeta` → `fkv_api::ObjectMeta` 的逐字段拷贝。
///
/// 这是 shard 把远端列举结果交给引擎 API 的唯一出口：迁移 host 只看得见
/// `fkv_api::ObjectMeta`，任何字段在这里被丢掉，数据面都不会报错。`key` /
/// `etag` / `size` 之外，sem-41 的 `last_modified`（C++ `mod_time`）必须原样
/// 透传；抽成独立函数是为了用固定时间点单测锁住拷贝（`dispatch_read` 本身
/// 依赖 Cloud 后端，纯函数更容易覆盖）。
fn api_meta(meta: fkv_object::ObjectMeta) -> ObjectMeta {
  ObjectMeta {
    key: meta.key,
    etag: meta.etag,
    size: meta.size,
    last_modified: meta.last_modified,
  }
}

/// 只读分发：只借 `&Store`，读任务与串行路径共用。
pub(crate) async fn dispatch_read(store: &ShardStore, req: &Request) -> Result<Response> {
  match req {
    Request::Get { tbl, key } => Ok(Response::Get(store.get(tbl, key).await?)),
    // 无 fixed buffer 的路径都走这里：无在飞读时的 OOM 内联重试（经
    // `handle`），以及 OOM 重排出的 `large=false` 读任务（io-11）。plain
    // 大值读只借 `&Store`，语义一致。
    Request::GetLarge { tbl, key } => Ok(Response::GetLarge(
      get_large_body(store, tbl, key, None).await?,
    )),
    Request::Scan {
      tbl,
      begin,
      end,
      max_entries,
      option,
    } => {
      let page = store
        .scan_page(
          tbl,
          begin,
          end,
          ScanOption {
            begin_inclusive: option.begin_inclusive,
            end_inclusive: option.end_inclusive,
            ..ScanOption::default()
          },
          // 统一为一个 entry 上限：显式分页优先，其次 Runtime::scan 的 max_entries。
          PageOption {
            page_entries: if option.page_entries != 0 {
              option.page_entries
            } else {
              *max_entries
            },
            page_size: option.page_size,
          },
        )
        .await?;
      Ok(Response::Scan(ScanReply {
        entry_li: page.entry_li,
        has_remaining: page.has_remaining,
      }))
    }
    Request::Floor { tbl, key } => {
      let floor = store.floor(tbl, key).await?;
      Ok(Response::Floor(FloorReply {
        key: floor.key,
        value: floor.value,
        metadata: floor.metadata,
        timestamp: floor.timestamp,
        expire_ts: floor.expire_ts,
      }))
    }
    Request::ListArchiveTags { tbl } => {
      let entry_li = store.list_archive_tags(tbl).await?;
      Ok(Response::ListArchiveTags(
        entry_li
          .into_iter()
          .map(|entry| ArchiveTagInfo {
            tag: entry.tag,
            term: entry.term,
          })
          .collect(),
      ))
    }
    Request::ListObject {
      prefix,
      token,
      max_keys,
      recursive,
      ..
    } => {
      // `tbl` 只用于路由；`prefix` 相对 store root（C++ `RemotePath`），返回名
      // 也相对它（均在 `StoreBackend::list_object` 内组合 / 剥离）。
      // `recursive` 对齐 C++ `ListObjectRequest::Recursive`（默认 false）。
      // Local / Mem 后端在 `StoreBackend::list_object` 内显式返回 InvalidArgs。
      let page = store
        .page_io()
        .inner()
        .list_object(prefix, token.as_deref(), *max_keys, *recursive)
        .await?;
      Ok(Response::ListObject(ListPage {
        meta_li: page.meta_li.into_iter().map(api_meta).collect(),
        next_token: page.next_token,
      }))
    }
    _ => Err(KvError::InvalidArgs),
  }
}

#[cfg(test)]
mod tests {
  use std::time::{Duration, UNIX_EPOCH};

  use super::api_meta;

  /// 固定时间点 `2026-01-01T00:00:00Z`：与 `fkv_object_s3` 的 S3 mock 用例
  /// （`MOCK_LAST_MODIFIED_SECS`）同一时刻，避免依赖系统时钟。
  const MODIFIED_SECS: u64 = 1_767_225_600;

  /// 引擎 API 的逐字段拷贝必须保留 `last_modified`（sem-41）：迁移 host 的
  /// `mod_time` 只看得到 `fkv_api::ObjectMeta`，这一层静默丢字段时 S3 层与
  /// 数据面测试都不会变红。
  #[test]
  fn api_meta_copies_every_field() {
    let modified = UNIX_EPOCH + Duration::from_secs(MODIFIED_SECS);
    let meta = api_meta(fkv_object::ObjectMeta {
      key: "demo.1/manifest_main_1".to_owned(),
      etag: "\"etag-1\"".to_owned(),
      size: 4096,
      last_modified: Some(modified),
    });
    assert_eq!(meta.key, "demo.1/manifest_main_1");
    assert_eq!(meta.etag, "\"etag-1\"");
    assert_eq!(meta.size, 4096);
    assert_eq!(meta.last_modified, Some(modified));
  }

  /// `None` 保持 `None`：Mem 后端与 common prefix 合成条目都没有修改时间，
  /// 不能在这里被补成其它值。
  #[test]
  fn api_meta_keeps_missing_last_modified() {
    let meta = api_meta(fkv_object::ObjectMeta {
      key: "sub/".to_owned(),
      etag: String::new(),
      size: 0,
      last_modified: None,
    });
    assert_eq!(meta.last_modified, None);
  }
}
