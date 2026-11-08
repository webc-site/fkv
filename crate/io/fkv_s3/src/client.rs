//! S3 最小客户端：GetObject / HeadObject / PutObject / DeleteObject /
//! ListObjectsV2（含 delimiter 与 continuation token）。
//!
//! 只用 [`crate::sig`] 签名 + [`crate::http`] 传输，不依赖任何异步运行时库
//! （HTTP 传输跑在 compio 上）。

use bytes::Bytes;
use fkv_error::{KvError, Result};
use quick_xml::events::Event;

use crate::{
  http::{Endpoint, request},
  sig::{SignInput, payload_hash, sign, uri_encode},
};

/// 客户端配置。
#[derive(Debug, Clone)]
pub struct S3Config {
  pub endpoint: Endpoint,
  pub bucket: String,
  pub region: String,
  pub access_key: String,
  pub secret_key: String,
  /// `true` 表示 path-style（`endpoint/bucket/key`），MinIO / rustfs 用这个。
  pub path_style: bool,
}

/// 列举结果中的一条对象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedObject {
  pub key: String,
  pub size: u64,
  pub etag: String,
}

/// 一页列举结果。
#[derive(Debug, Clone, Default)]
pub struct ListPage {
  pub object_li: Vec<ListedObject>,
  pub prefix_li: Vec<String>,
  pub next_token: Option<String>,
  pub truncated: bool,
}

/// 对象头信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
  pub etag: String,
  pub size: u64,
}

pub struct S3Client {
  cfg: S3Config,
}

impl S3Client {
  pub fn new(cfg: S3Config) -> Self {
    Self { cfg }
  }

  /// 主机名与请求目标：path-style 与 virtual-host 两种形态。
  fn host_and_path(&self, key: Option<&str>) -> (String, String) {
    if self.cfg.path_style {
      let mut path = format!("/{}", self.cfg.bucket);
      if let Some(key) = key {
        path.push('/');
        path.push_str(key);
      }
      (self.cfg.endpoint.host.clone(), path)
    } else {
      let host = format!("{}.{}", self.cfg.bucket, self.cfg.endpoint.host);
      let path = match key {
        Some(key) => format!("/{key}"),
        None => "/".to_owned(),
      };
      (host, path)
    }
  }

  async fn send(
    &self,
    method: &str,
    key: Option<&str>,
    query_li: &[(String, String)],
    extra_header_li: Vec<(String, String)>,
    body: Bytes,
  ) -> Result<crate::http::Response> {
    let (host, path) = self.host_and_path(key);
    let amz_date = amz_date();
    let hash = payload_hash(&body);

    let mut header_li = vec![
      ("host".to_owned(), host.clone()),
      ("x-amz-content-sha256".to_owned(), hash.clone()),
      ("x-amz-date".to_owned(), amz_date.clone()),
    ];
    header_li.extend(extra_header_li);

    let signed = sign(&SignInput {
      method,
      path: &path,
      query_li,
      header_li: &header_li,
      payload_hash: &hash,
      amz_date: &amz_date,
      region: &self.cfg.region,
      access_key: &self.cfg.access_key,
      secret_key: &self.cfg.secret_key,
    });
    header_li.push(("authorization".to_owned(), signed.authorization.clone()));

    let query = if query_li.is_empty() {
      String::new()
    } else {
      let mut pairs: Vec<String> = query_li
        .iter()
        .map(|(k, v)| format!("{}={}", uri_encode(k, true), uri_encode(v, true)))
        .collect();
      pairs.sort();
      format!("?{}", pairs.join("&"))
    };
    let target = format!("{}{}", uri_encode(&path, false), query);
    request(&self.cfg.endpoint, method, &target, &header_li, &body).await
  }

  /// 读对象；不存在返回 `None`。
  pub async fn get(&self, key: &str) -> Result<Option<Bytes>> {
    let response = self
      .send("GET", Some(key), &[], Vec::new(), Bytes::new())
      .await?;
    match response.status {
      200 => Ok(Some(response.body)),
      404 => Ok(None),
      _ => Err(map_status(response.status)),
    }
  }

  /// 读对象头；不存在返回 `None`。
  pub async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    let response = self
      .send("HEAD", Some(key), &[], Vec::new(), Bytes::new())
      .await?;
    match response.status {
      200 => {
        let etag = response
          .header("etag")
          .map(|value| value.trim_matches('"').to_owned())
          .unwrap_or_default();
        let size = response
          .header("content-length")
          .and_then(|value| value.parse::<u64>().ok())
          .unwrap_or(0);
        Ok(Some(ObjectMeta { etag, size }))
      }
      404 => Ok(None),
      _ => Err(map_status(response.status)),
    }
  }

  /// 写整对象。`if_match` / `if_none_match` 走 S3 的条件写头。
  pub async fn put(
    &self,
    key: &str,
    body: Bytes,
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<Option<String>> {
    let mut header_li = Vec::new();
    if let Some(etag) = if_match {
      header_li.push(("if-match".to_owned(), etag.to_owned()));
    }
    if if_none_match {
      header_li.push(("if-none-match".to_owned(), "*".to_owned()));
    }
    let response = self.send("PUT", Some(key), &[], header_li, body).await?;
    match response.status {
      200 | 201 | 204 => Ok(
        response
          .header("etag")
          .map(|value| value.trim_matches('"').to_owned()),
      ),
      412 => Err(KvError::ExpiredTerm),
      _ => Err(map_status(response.status)),
    }
  }

  /// 同 [`Self::put`]，额外返回 ETag（用于条件写的后续比对）。
  pub async fn get_with_etag(&self, key: &str) -> Result<Option<(Bytes, String)>> {
    let response = self
      .send("GET", Some(key), &[], Vec::new(), Bytes::new())
      .await?;
    match response.status {
      200 => {
        let etag = response
          .header("etag")
          .map(|value| value.trim_matches('"').to_owned())
          .unwrap_or_default();
        Ok(Some((response.body, etag)))
      }
      404 => Ok(None),
      _ => Err(map_status(response.status)),
    }
  }

  /// 删除对象（S3 的 DELETE 对不存在也返回 204）。
  pub async fn delete(&self, key: &str) -> Result<()> {
    let response = self
      .send("DELETE", Some(key), &[], Vec::new(), Bytes::new())
      .await?;
    match response.status {
      200 | 204 => Ok(()),
      _ => Err(map_status(response.status)),
    }
  }

  /// ListObjectsV2 一页。
  pub async fn list_page(
    &self,
    prefix: &str,
    token: Option<&str>,
    max_keys: usize,
    delimiter: Option<&str>,
  ) -> Result<ListPage> {
    let mut query_li = vec![
      ("list-type".to_owned(), "2".to_owned()),
      ("max-keys".to_owned(), max_keys.to_string()),
    ];
    if !prefix.is_empty() {
      query_li.push(("prefix".to_owned(), prefix.to_owned()));
    }
    if let Some(token) = token {
      query_li.push(("continuation-token".to_owned(), token.to_owned()));
    }
    if let Some(delimiter) = delimiter {
      query_li.push(("delimiter".to_owned(), delimiter.to_owned()));
    }
    let response = self
      .send("GET", None, &query_li, Vec::new(), Bytes::new())
      .await?;
    match response.status {
      200 => parse_list(&response.body),
      _ => Err(map_status(response.status)),
    }
  }
}

pub fn map_status(status: u16) -> KvError {
  match status {
    403 => KvError::ResourceMissing,
    404 => KvError::ResourceMissing,
    409 => KvError::TryAgain,
    412 => KvError::ExpiredTerm,
    429 | 500 | 502 | 503 | 504 => KvError::TryAgain,
    507 => KvError::OssInsufficientStorage,
    _ => KvError::CloudErr,
  }
}

/// `YYYYMMDDTHHMMSSZ`（UTC）。签名要求秒级时间，取系统时间即可。
fn amz_date() -> String {
  let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs();
  let (year, month, day) = civil_from_days((now / 86_400) as i64);
  let secs_of_day = now % 86_400;
  format!(
    "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
    secs_of_day / 3600,
    (secs_of_day % 3600) / 60,
    secs_of_day % 60
  )
}

/// 由 Unix 天数换算公历年月日（Howard Hinnant 的算法）。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
  let z = days + 719_468;
  let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
  let doe = (z - era * 146_097) as u64;
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let y = yoe as i64 + era * 400;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
  let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
  (if m <= 2 { y + 1 } else { y }, m, d)
}

fn parse_list(body: &[u8]) -> Result<ListPage> {
  let mut reader = quick_xml::Reader::from_reader(body);
  let mut page = ListPage::default();
  let mut current: Option<ListedObject> = None;
  let mut current_prefix: Option<String> = None;
  let mut field = String::new();
  let mut truncated = false;
  loop {
    match reader.read_event().map_err(|_| KvError::Corrupted)? {
      Event::Start(event) => {
        field = event.name().as_ref().to_owned();
        match field.as_str() {
          "Contents" => {
            current = Some(ListedObject {
              key: String::new(),
              size: 0,
              etag: String::new(),
            })
          }
          "CommonPrefixes" => current_prefix = Some(String::new()),
          _ => {}
        }
      }
      Event::End(event) => {
        let name = event.name().as_ref().to_owned();
        if name == "Contents"
          && let Some(object) = current.take()
        {
          page.object_li.push(object);
        }
        if name == "CommonPrefixes"
          && let Some(prefix) = current_prefix.take()
        {
          page.prefix_li.push(prefix);
        }
        // 元素结束后必须清掉字段名：否则紧随其后的缩进空白会被当成该字段的值。
        field.clear();
      }
      Event::Text(text) => {
        let raw = text.as_ref().to_owned();
        if raw.trim().is_empty() || field.is_empty() {
          continue;
        }
        let value = quick_xml::escape::unescape(&raw)
          .map_err(|_| KvError::Corrupted)?
          .into_owned();
        match field.as_str() {
          "Key" => {
            if let Some(object) = current.as_mut() {
              object.key = value;
            }
          }
          "Size" => {
            if let Some(object) = current.as_mut() {
              object.size = value.parse().unwrap_or(0);
            }
          }
          "ETag" => {
            if let Some(object) = current.as_mut() {
              object.etag = value.trim_matches('"').to_owned();
            }
          }
          "Prefix" => {
            if let Some(prefix) = current_prefix.as_mut() {
              *prefix = value;
            }
          }
          "NextContinuationToken" => page.next_token = Some(value),
          "IsTruncated" => truncated = value.eq_ignore_ascii_case("true"),
          _ => {}
        }
      }
      Event::Eof => break,
      _ => {}
    }
  }
  page.truncated = truncated;
  Ok(page)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_list_objects_v2_page() {
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>bucket</Name>
  <Prefix>data/</Prefix>
  <KeyCount>2</KeyCount>
  <MaxKeys>1000</MaxKeys>
  <IsTruncated>true</IsTruncated>
  <NextContinuationToken>token-1</NextContinuationToken>
  <Contents>
    <Key>data/a.bin</Key><Size>4096</Size><ETag>"etag-a"</ETag>
  </Contents>
  <Contents>
    <Key>data/b.bin</Key><Size>8192</Size><ETag>"etag-b"</ETag>
  </Contents>
  <CommonPrefixes><Prefix>data/sub/</Prefix></CommonPrefixes>
</ListBucketResult>"#;
    let page = parse_list(xml).expect("parse");
    assert_eq!(page.object_li.len(), 2);
    assert_eq!(page.object_li[0].key, "data/a.bin");
    assert_eq!(page.object_li[0].size, 4096);
    assert_eq!(page.object_li[0].etag, "etag-a");
    assert_eq!(page.object_li[1].etag, "etag-b");
    assert_eq!(page.prefix_li, vec!["data/sub/".to_owned()]);
    assert_eq!(page.next_token.as_deref(), Some("token-1"));
    assert!(page.truncated);
  }

  #[test]
  fn parses_empty_list_page() {
    let xml =
      br#"<ListBucketResult><Name>b</Name><IsTruncated>false</IsTruncated></ListBucketResult>"#;
    let page = parse_list(xml).expect("parse");
    assert!(page.object_li.is_empty());
    assert!(!page.truncated);
    assert!(page.next_token.is_none());
  }

  #[test]
  fn amz_date_has_iso_shape() {
    let date = amz_date();
    assert_eq!(date.len(), 16, "{date}");
    assert!(date.ends_with('Z'));
    assert_eq!(&date[8..9], "T");
  }
}
