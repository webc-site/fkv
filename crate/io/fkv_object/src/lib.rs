//! 对象存储抽象 + 内存实现。

use core::{future::Future, mem::replace};
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use fkv_error::{KvError, Result};
use gxhash::HashMap;
use parking_lot::Mutex;

/// key -> (内容, etag)。内容存 [`Bytes`]，读写路径都不需要整对象拷贝。
type ObjectMap = HashMap<String, (Bytes, String)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
  pub key: String,
  pub etag: String,
  pub size: u64,
}

/// 一页列举结果；`next_token` 为下一页的起始 token（上次返回的最后一个 key）。
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
}

#[derive(Debug, Clone, Default)]
pub struct MemObjectStore {
  map: Arc<Mutex<ObjectMap>>,
  version: Arc<Mutex<u64>>,
  /// 经 `put_range` 写入的字节数（测试用；验证增量上传）。
  range_put_bytes: Arc<Mutex<u64>>,
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
      })
      .collect();
    meta_li.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(meta_li)
  }
}
