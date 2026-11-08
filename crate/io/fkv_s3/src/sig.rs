//! AWS Signature V4（服务 s3）：纯函数、无运行时依赖。
//!
//! 只实现 S3 需要的形态：header 签名（不是 presigned URL），payload 走
//! `x-amz-content-sha256` 显式给哈希（S3 强制要求）。

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// 一次请求的签名输入。
pub struct SignInput<'a> {
  /// `GET` / `PUT` / `DELETE` / `HEAD`。
  pub method: &'a str,
  /// 未编码的路径，如 `/bucket/key`（内部按 RFC3986 编码，`/` 保留）。
  pub path: &'a str,
  /// 已排序的查询串片段（未编码的 key/value 对）。
  pub query_li: &'a [(String, String)],
  /// 参与签名的 header（小写名，值未 trim），至少包含 `host`。
  pub header_li: &'a [(String, String)],
  /// 载荷的 SHA-256（十六进制小写）。
  pub payload_hash: &'a str,
  /// `YYYYMMDDTHHMMSSZ`。
  pub amz_date: &'a str,
  pub region: &'a str,
  pub access_key: &'a str,
  pub secret_key: &'a str,
}

/// 签名结果：`Authorization` 头的值与签名范围 / 签名 header 列表。
pub struct Signed {
  pub authorization: String,
  pub signed_header_li: Vec<String>,
}

/// RFC3986 编码：unreserved 之外的字节都编码成大写 `%XX`。
pub fn uri_encode(input: &str, encode_slash: bool) -> String {
  let mut out = String::with_capacity(input.len());
  for byte in input.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
      b'/' if !encode_slash => out.push('/'),
      _ => out.push_str(&format!("%{byte:02X}")),
    }
  }
  out
}

fn sha256_hex(bytes: &[u8]) -> String {
  hex::encode(Sha256::digest(bytes))
}

/// 载荷哈希（S3 用 `UNSIGNED-PAYLOAD` 之外的显式哈希；空体即空串哈希）。
pub fn payload_hash(bytes: &[u8]) -> String {
  sha256_hex(bytes)
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
  let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 接受任意长度密钥");
  mac.update(data);
  mac.finalize().into_bytes().to_vec()
}

/// 计算 SigV4 签名。
pub fn sign(input: &SignInput<'_>) -> Signed {
  // 规范查询串：按编码后的 key 排序（AWS 规则：先按名再按值）。
  let mut query_li: Vec<(String, String)> = input
    .query_li
    .iter()
    .map(|(k, v)| (uri_encode(k, true), uri_encode(v, true)))
    .collect();
  query_li.sort();
  let canonical_query = query_li
    .iter()
    .map(|(k, v)| format!("{k}={v}"))
    .collect::<Vec<_>>()
    .join("&");

  // 规范 header：小写名、值 trim + 内部空白折叠、按名排序。
  let mut header_li: Vec<(String, String)> = input
    .header_li
    .iter()
    .map(|(name, value)| {
      let mut trimmed = String::with_capacity(value.len());
      let mut prev_space = false;
      for ch in value.trim().chars() {
        if ch == ' ' || ch == '\t' {
          if !prev_space {
            trimmed.push(' ');
          }
          prev_space = true;
        } else {
          trimmed.push(ch);
          prev_space = false;
        }
      }
      (name.to_ascii_lowercase(), trimmed)
    })
    .collect();
  header_li.sort_by(|a, b| a.0.cmp(&b.0));
  let canonical_header = header_li
    .iter()
    .map(|(name, value)| format!("{name}:{value}\n"))
    .collect::<String>();
  let signed_header_li: Vec<String> = header_li.iter().map(|(name, _)| name.clone()).collect();
  let signed_headers = signed_header_li.join(";");

  let canonical_request = format!(
    "{}\n{}\n{}\n{}\n{}\n{}",
    input.method,
    uri_encode(input.path, false),
    canonical_query,
    canonical_header,
    signed_headers,
    input.payload_hash
  );

  let date = &input.amz_date[..8];
  let scope = format!("{}/{}/s3/aws4_request", date, input.region);
  let string_to_sign = format!(
    "AWS4-HMAC-SHA256\n{}\n{}\n{}",
    input.amz_date,
    scope,
    sha256_hex(canonical_request.as_bytes())
  );

  let key = hmac(
    format!("AWS4{}", input.secret_key).as_bytes(),
    date.as_bytes(),
  );
  let key = hmac(&key, input.region.as_bytes());
  let key = hmac(&key, b"s3");
  let key = hmac(&key, b"aws4_request");
  let signature = hex::encode(hmac(&key, string_to_sign.as_bytes()));

  Signed {
    authorization: format!(
      "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
      input.access_key, scope, signed_headers, signature
    ),
    signed_header_li,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// AWS 文档示例（GET Object，us-east-1，2013-05-24T00:00:00Z）。
  #[test]
  fn matches_aws_get_object_vector() {
    let signed = sign(&SignInput {
      method: "GET",
      path: "/test.txt",
      query_li: &[],
      header_li: &[
        (
          "host".to_owned(),
          "examplebucket.s3.amazonaws.com".to_owned(),
        ),
        ("range".to_owned(), "bytes=0-9".to_owned()),
        (
          "x-amz-content-sha256".to_owned(),
          "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
        ),
        ("x-amz-date".to_owned(), "20130524T000000Z".to_owned()),
      ],
      payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      amz_date: "20130524T000000Z",
      region: "us-east-1",
      access_key: "AKIAIOSFODNN7EXAMPLE",
      secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    });
    assert!(
      signed
        .authorization
        .ends_with("Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"),
      "{}",
      signed.authorization
    );
  }

  #[test]
  fn uri_encode_keeps_slash_only_when_allowed() {
    assert_eq!(uri_encode("/a b/c", false), "/a%20b/c");
    assert_eq!(uri_encode("/a b/c", true), "%2Fa%20b%2Fc");
    assert_eq!(uri_encode("a~b-c_d.e", true), "a~b-c_d.e");
  }
}
