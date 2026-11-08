//! 对象存储抽象 + 内存实现。

use core::{future::Future, mem::replace};
use std::{
  collections::{BTreeMap, btree_map::Entry},
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::SystemTime,
};

use bytes::{Bytes, BytesMut};
use fkv_error::{KvError, Result};
use gxhash::HashMap;
use parking_lot::Mutex;

/// key -> (内容, etag)。内容存 [`Bytes`]，读写路径都不需要整对象拷贝。
type ObjectMap = HashMap<String, (Bytes, String)>;

/// 非递归列举的扫描页大小：默认实现在本地按目录归并，分块读取避免一次
/// 物化整个 prefix。只影响单次 `list_page` 的批量，不改变对外分页语义。
const LEVEL_SCAN_CHUNK: usize = 1024;

/// 把 key 归并到 `prefix` 下的当前层条目。
///
/// 返回 `(条目 key, 是否 common prefix)`：`prefix` 之后第一段出现 `/` 时，
/// 条目是到该 `/` 为止的 common prefix；否则条目就是对象 key 本身。这与
/// S3 ListObjectsV2 `prefix` + `delimiter=/` 的 `Contents` / `CommonPrefixes`
/// 合并规则一致（等价 C++ `ListObjectRequest` 默认 `recursive=false`）。
fn level_entry<'k>(prefix: &str, key: &'k str) -> Option<(&'k str, bool)> {
  let rest = key.strip_prefix(prefix)?;
  match rest.find('/') {
    Some(idx) => Some((&key[..prefix.len() + idx + 1], true)),
    None => Some((key, false)),
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
  pub key: String,
  pub etag: String,
  pub size: u64,
  /// 对象最后修改时间（S3 `LastModified`）。
  ///
  /// 对齐 C++ `utils::CloudObjectInfo::mod_time`（C++ 保留 S3 返回的原始
  /// 时间字符串，这里保留同一时间点的类型化表示）。`None` 表示该后端不提供：
  /// [`MemObjectStore`] 不记录修改时间；非递归列举的 common prefix 是合成
  /// 条目，也没有真实对象的修改时间。
  pub last_modified: Option<SystemTime>,
}

/// 一页列举结果。
///
/// `meta_li` 按 key 字节序升序；`next_token` 是下一页的起始 token（本页最后
/// 一个条目的后端绝对 key），对调用方**不透明**：只能原样回传给下一次
/// [`ObjectStore::list_page`] / [`ObjectStore::list_level_page`]，不得解析或
/// 跨实现复用（C++ `ListObjectRequest` 的 token 是 provider continuation
/// token，两者形态不同）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPage {
  pub meta_li: Vec<ObjectMeta>,
  pub next_token: Option<String>,
}

pub trait ObjectStore {
  /// 返回引用计数的 [`Bytes`]：跨线程通道只搬指针，调用方需要 `Vec` 时再物化。
  fn get<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<Option<Bytes>>> + 'a;

  /// 同一次读取同时返回内容与 etag。支持单请求的后端应覆盖；
  /// 默认实现退化为 get + head（value 与 etag 不同源）。
  fn get_versioned<'a>(
    &'a self,
    key: &'a str,
  ) -> impl Future<Output = Result<Option<(Bytes, String)>>> + 'a {
    async move {
      let Some(bytes) = self.get(key).await? else {
        return Ok(None);
      };
      let Some(meta) = self.head(key).await? else {
        return Ok(None);
      };
      Ok(Some((bytes, meta.etag)))
    }
  }

  /// 写入整对象。`value` 移交所有权（`Bytes::from(vec)` 零拷贝），
  /// 后端因此无需在入队 / 发起请求前再拷一份。
  fn put<'a>(
    &'a self,
    key: &'a str,
    value: Bytes,
    if_match: Option<&'a str>,
    if_none_match: bool,
  ) -> impl Future<Output = Result<String>> + 'a;

  /// 区间写：默认读-改-写整对象；支持 range PUT 的后端可覆盖以减少上传字节。
  fn put_range<'a>(
    &'a self,
    key: &'a str,
    offset: u64,
    bytes: &'a [u8],
  ) -> impl Future<Output = Result<()>> + 'a {
    async move {
      let mut base = BytesMut::from(self.get(key).await?.as_deref().unwrap_or_default());
      let start = offset as usize;
      let end = start.checked_add(bytes.len()).ok_or(KvError::InvalidArgs)?;
      if base.len() < end {
        base.resize(end, 0);
      }
      base[start..end].copy_from_slice(bytes);
      // 已自行 RMW 合并，回写必须覆盖已存在对象。
      self.put(key, base.freeze(), None, false).await.map(|_| ())
    }
  }

  /// 后端是否支持真正的区间写；false 时调用方应整对象上传。
  fn supports_put_range(&self) -> bool {
    false
  }

  fn head<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<Option<ObjectMeta>>> + 'a;

  fn delete<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<()>> + 'a;

  /// 列举整个 prefix 下的对象，key 按字节序升序。
  ///
  /// 便捷方法，内部会把整个 prefix 物化进 `Vec`；百万对象级 bucket 下
  /// 内存与首字节延迟都不可控。新代码优先用 [`ObjectStore::list_page`]。
  fn list<'a>(&'a self, prefix: &'a str) -> impl Future<Output = Result<Vec<ObjectMeta>>> + 'a;

  /// 分页列举 `prefix` 下的对象。
  ///
  /// 契约（所有后端必须遵守）：
  /// - key 按**字节序升序**返回，与 S3 ListObjectsV2 一致；
  /// - `prefix` 是**递归**前缀匹配，没有目录 / 分隔符语义；
  /// - `token` 取上一页的 `next_token`，语义为「只返回 key 严格大于 token
  ///   的对象」，因此 token 就是上一页最后一个 key；
  /// - `max_keys == 0` 表示不限数量；
  /// - 本页之后仍有对象时 `next_token` 为 `Some(本页最后一个 key)`，
  ///   调用方据此继续翻页；已到末尾时为 `None`（即使本页恰好取满）。
  ///
  /// 默认实现基于 `list` 过滤 + 截断，会物化整个 prefix；支持流式列举的
  /// 后端应覆盖本方法以提前终止。
  fn list_page<'a>(
    &'a self,
    prefix: &'a str,
    token: Option<&'a str>,
    max_keys: usize,
  ) -> impl Future<Output = Result<ListPage>> + 'a {
    async move {
      // 默认实现保证「严格大于 token」契约，后端覆盖时同样要满足。
      let mut meta_li = self.list(prefix).await?;
      if let Some(token) = token {
        meta_li.retain(|meta| meta.key.as_str() > token);
      }
      if max_keys != 0 && meta_li.len() > max_keys {
        let next_token = meta_li.get(max_keys - 1).map(|meta| meta.key.clone());
        meta_li.truncate(max_keys);
        return Ok(ListPage {
          meta_li,
          next_token,
        });
      }
      Ok(ListPage {
        meta_li,
        next_token: None,
      })
    }
  }

  /// 分页列举 `prefix` 下的**当前层**条目（非递归）。
  ///
  /// 对齐 C++ `ListObjectRequest` 的默认形态（`recursive = false`）：等价
  /// S3 ListObjectsV2 带 `delimiter=/`，只返回 `prefix` 下不跨目录的对象，
  /// 以及以 `/` 结尾的 common prefixes（合成条目：`etag` 为空、`size == 0`），
  /// 两者按 key 字节序合并。需要全部后代时用 [`ObjectStore::list_page`]
  /// （等价 C++ `SetRecursive(true)`）。
  ///
  /// 分页契约同 [`ObjectStore::list_page`]，但 token 落在「条目」而不是原始
  /// key 上：`token` 可能是上一页返回的 common prefix，下一页会跳过它及其
  /// 全部后代；`max_keys == 0` 表示不限数量。
  ///
  /// 默认实现按 [`ObjectStore::list_page`] 分块扫描并在本地归并（只用于没有
  /// 原生 delimiter 分页接口的后端）；S3 / Mem 后端已覆盖本方法，分别下推
  /// `delimiter=/` 与单次归并，扫描量不再随分块次数放大。
  fn list_level_page<'a>(
    &'a self,
    prefix: &'a str,
    token: Option<&'a str>,
    max_keys: usize,
  ) -> impl Future<Output = Result<ListPage>> + 'a {
    async move {
      let mut meta_li: Vec<ObjectMeta> = Vec::new();
      let mut cursor = token.map(str::to_owned);
      let mut last_entry: Option<String> = None;
      let mut has_more = false;

      'scan: loop {
        let page = self
          .list_page(prefix, cursor.as_deref(), LEVEL_SCAN_CHUNK)
          .await?;
        let page_next = page.next_token.clone();
        for meta in page.meta_li {
          let Some((entry_key, is_prefix)) = level_entry(prefix, &meta.key) else {
            continue;
          };
          // 上一页最后返回的条目（可能是 common prefix）及其后代都不重复。
          if token.is_some_and(|floor| entry_key <= floor) {
            continue;
          }
          // 同一 common prefix 的后续 key 归并到已返回的条目。
          if last_entry.as_deref() == Some(entry_key) {
            continue;
          }
          if max_keys != 0 && meta_li.len() == max_keys {
            // 还有下一个条目：token 落在本页最后一个条目上。
            has_more = true;
            break 'scan;
          }
          last_entry = Some(entry_key.to_owned());
          if is_prefix {
            meta_li.push(ObjectMeta {
              key: entry_key.to_owned(),
              etag: String::new(),
              size: 0,
              last_modified: None,
            });
          } else {
            meta_li.push(meta);
          }
        }
        match page_next {
          Some(next) => cursor = Some(next),
          None => break,
        }
      }

      let next_token = if has_more { last_entry } else { None };
      Ok(ListPage {
        meta_li,
        next_token,
      })
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct MemObjectStore {
  map: Arc<Mutex<ObjectMap>>,
  version: Arc<Mutex<u64>>,
  /// 经 `put_range` 写入的字节数（测试用；验证增量上传）。
  range_put_bytes: Arc<Mutex<u64>>,
  /// 非递归列举扫描过的 key 数（测试用；验证单次归并而非按块重复物化）。
  level_scan_keys: Arc<AtomicU64>,
}

impl MemObjectStore {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn len(&self) -> usize {
    self.map.lock().len()
  }

  pub fn is_empty(&self) -> bool {
    self.map.lock().is_empty()
  }

  pub fn range_put_bytes(&self) -> u64 {
    *self.range_put_bytes.lock()
  }

  /// 累计的非递归列举扫描 key 数；单次 `list_level_page` 应只增加前缀下的
  /// 对象数，而不是「分块数 × 对象数」。
  pub fn level_scan_keys(&self) -> u64 {
    self.level_scan_keys.load(Ordering::Relaxed)
  }
}

impl ObjectStore for MemObjectStore {
  fn supports_put_range(&self) -> bool {
    true
  }

  async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    Ok(self.map.lock().get(key).map(|(data, _)| data.clone()))
  }

  async fn get_versioned(&self, key: &str) -> Result<Option<(Bytes, String)>> {
    Ok(
      self
        .map
        .lock()
        .get(key)
        .map(|(data, etag)| (data.clone(), etag.clone())),
    )
  }

  async fn put(
    &self,
    key: &str,
    value: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    let mut map = self.map.lock();
    let current = map.get(key);
    if if_none_match && current.is_some() {
      return Err(KvError::AlreadyExists);
    }
    if let Some(if_match) = if_match {
      match current {
        Some((_, etag)) if etag == if_match => {}
        _ => return Err(KvError::Busy),
      }
    }
    let mut version = self.version.lock();
    *version += 1;
    let etag = format!("{:016x}", *version);
    map.insert(key.to_owned(), (value, etag.clone()));
    Ok(etag)
  }

  async fn put_range(&self, key: &str, offset: u64, bytes: &[u8]) -> Result<()> {
    let mut map = self.map.lock();
    let start = offset as usize;
    let end = start.checked_add(bytes.len()).ok_or(KvError::InvalidArgs)?;
    let mut version = self.version.lock();
    *version += 1;
    let etag = format!("{:016x}", *version);
    let entry = map
      .entry(key.to_owned())
      .or_insert_with(|| (Bytes::new(), etag.clone()));
    // 独占缓冲时直接复用（零拷贝）；有其它读者（get 克隆过）时才复制一份。
    let base = replace(&mut entry.0, Bytes::new());
    let mut buf = base
      .try_into_mut()
      .unwrap_or_else(|base| BytesMut::from(&base[..]));
    if buf.len() < end {
      buf.resize(end, 0);
    }
    buf[start..end].copy_from_slice(bytes);
    entry.0 = buf.freeze();
    entry.1 = etag;
    *self.range_put_bytes.lock() += bytes.len() as u64;
    Ok(())
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    Ok(self.map.lock().get(key).map(|(data, etag)| ObjectMeta {
      key: key.to_owned(),
      etag: etag.clone(),
      size: data.len() as u64,
      last_modified: None,
    }))
  }

  async fn delete(&self, key: &str) -> Result<()> {
    self.map.lock().remove(key);
    Ok(())
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    let mut meta_li: Vec<ObjectMeta> = self
      .map
      .lock()
      .iter()
      .filter(|(key, _)| key.starts_with(prefix))
      .map(|(key, (data, etag))| ObjectMeta {
        key: key.clone(),
        etag: etag.clone(),
        size: data.len() as u64,
        last_modified: None,
      })
      .collect();
    meta_li.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(meta_li)
  }

  /// 单次扫描归并当前层条目：只遍历 prefix 下全部对象一次，用有序 map 去重
  /// common prefixes，复杂度 `O(D log L)`（D = 后代对象数，L = 当前层条目数），
  /// 不再按 [`LEVEL_SCAN_CHUNK`] 分块重复物化整个 prefix（D = 10 万、L = 100
  /// 时从 98 次全量列举降为 1 次）。
  async fn list_level_page(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
  ) -> Result<ListPage> {
    let map = self.map.lock();
    // entry key -> Some((内容, etag)) 表示当前层对象，None 表示 common prefix。
    // 同一 common prefix 的后代归并到同一 key；对象 key 唯一。
    let mut entries: BTreeMap<&str, Option<(&Bytes, &String)>> = BTreeMap::new();
    let mut scanned = 0u64;
    for (key, (data, etag)) in map.iter() {
      let Some((entry_key, is_prefix)) = level_entry(prefix, key) else {
        continue;
      };
      scanned += 1;
      if token.is_some_and(|floor| entry_key <= floor) {
        continue;
      }
      match entries.entry(entry_key) {
        // 目录标记对象（key 恰为 common prefix）与后代并存时以 common prefix
        // 为准，对齐 S3 ListObjectsV2 的 rollup（该对象不会出现在 Contents）。
        Entry::Occupied(mut slot) => {
          if is_prefix {
            slot.insert(None);
          }
        }
        Entry::Vacant(slot) => {
          slot.insert((!is_prefix).then_some((data, etag)));
        }
      }
    }
    self.level_scan_keys.fetch_add(scanned, Ordering::Relaxed);

    let keep = if max_keys == 0 {
      entries.len()
    } else {
      entries.len().min(max_keys)
    };
    let has_more = keep < entries.len();
    let mut meta_li = Vec::with_capacity(keep);
    let mut last_key = None;
    for (entry_key, object) in entries.into_iter().take(keep) {
      last_key = Some(entry_key);
      meta_li.push(match object {
        Some((data, etag)) => ObjectMeta {
          key: entry_key.to_owned(),
          etag: etag.clone(),
          size: data.len() as u64,
          last_modified: None,
        },
        None => ObjectMeta {
          key: entry_key.to_owned(),
          etag: String::new(),
          size: 0,
          last_modified: None,
        },
      });
    }
    Ok(ListPage {
      meta_li,
      // 恰好取满但无后续时 last_key 不导出，保持「None = 已到末尾」契约。
      next_token: if has_more {
        last_key.map(str::to_owned)
      } else {
        None
      },
    })
  }
}
