//! 对象存储抽象 + 内存实现。

use core::future::Future;
use std::sync::Arc;

use fkv_error::{KvError, Result};
use gxhash::HashMap;
use parking_lot::Mutex;

type ObjectMap = HashMap<String, (Vec<u8>, String)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
  pub key: String,
  pub etag: String,
  pub size: u64,
}

pub trait ObjectStore {
  fn get<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<Option<Vec<u8>>>> + 'a;

  fn put<'a>(
    &'a self,
    key: &'a str,
    value: &'a [u8],
    if_match: Option<&'a str>,
    if_none_match: bool,
  ) -> impl Future<Output = Result<String>> + 'a;

  fn head<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<Option<ObjectMeta>>> + 'a;

  fn delete<'a>(&'a self, key: &'a str) -> impl Future<Output = Result<()>> + 'a;

  fn list<'a>(&'a self, prefix: &'a str) -> impl Future<Output = Result<Vec<ObjectMeta>>> + 'a;
}

#[derive(Debug, Clone, Default)]
pub struct MemObjectStore {
  map: Arc<Mutex<ObjectMap>>,
  version: Arc<Mutex<u64>>,
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
}

impl ObjectStore for MemObjectStore {
  async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(self.map.lock().get(key).map(|(data, _)| data.clone()))
  }

  async fn put(
    &self,
    key: &str,
    value: &[u8],
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
    map.insert(key.to_owned(), (value.to_vec(), etag.clone()));
    Ok(etag)
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
