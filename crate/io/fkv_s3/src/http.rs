//! 最小 HTTP/1.1 客户端（compio 传输层）。
//!
//! 只覆盖 S3 需要的形态：请求带定长 body、响应按 `Content-Length` 或
//! `Transfer-Encoding: chunked` 读取。不做连接池、不做 HTTP/2——S3 的每次
//! 对象操作都是一次短请求，连接复用留到后面按需加。

use std::sync::Arc;

use bytes::Bytes;
use compio::{
  io::{AsyncRead, AsyncWriteExt},
  net::TcpStream,
};
use fkv_error::{KvError, Result};

/// 连接目标。
#[derive(Debug, Clone)]
pub struct Endpoint {
  pub host: String,
  pub port: u16,
  /// `true` 走 TLS（https）。
  pub tls: bool,
  /// TLS SNI / 证书校验用的域名。
  pub sni: String,
}

impl Endpoint {
  /// 从 `http(s)://host[:port]` 解析；非 http(s) 视为错误。
  pub fn parse(url: &str) -> Result<Self> {
    let (tls, rest) = if let Some(rest) = url.strip_prefix("https://") {
      (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
      (false, rest)
    } else {
      return Err(KvError::InvalidArgs);
    };
    let host_port = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match host_port.rsplit_once(':') {
      Some((host, port)) => (
        host.to_owned(),
        port.parse::<u16>().map_err(|_| KvError::InvalidArgs)?,
      ),
      None => (host_port.to_owned(), if tls { 443 } else { 80 }),
    };
    if host.is_empty() {
      return Err(KvError::InvalidArgs);
    }
    Ok(Self {
      sni: host.clone(),
      host,
      port,
      tls,
    })
  }
}

/// 一次响应。
#[derive(Debug)]
pub struct Response {
  pub status: u16,
  pub header_li: Vec<(String, String)>,
  pub body: Bytes,
}

impl Response {
  pub fn header(&self, name: &str) -> Option<&str> {
    self
      .header_li
      .iter()
      .find(|(key, _)| key.eq_ignore_ascii_case(name))
      .map(|(_, value)| value.as_str())
  }
}

/// 传输对象：明文或 TLS，读接口统一。
enum Stream {
  Plain(TcpStream),
  Tls(Box<compio_tls::TlsStream<TcpStream>>),
}

impl Stream {
  async fn write_all_bytes(&mut self, bytes: &[u8]) -> Result<()> {
    let buf = bytes.to_vec();
    let result = match self {
      Stream::Plain(stream) => stream.write_all(buf).await,
      Stream::Tls(stream) => stream.write_all(buf).await,
    };
    result.0.map_err(|_| KvError::IoFail)
  }

  async fn read_into(&mut self, dst: &mut Vec<u8>, max: usize) -> Result<usize> {
    let buf = vec![0u8; max];
    let result = match self {
      Stream::Plain(stream) => stream.read(buf).await,
      Stream::Tls(stream) => stream.read(buf).await,
    };
    let n = result.0.map_err(|_| KvError::IoFail)?;
    let buf = result.1;
    dst.extend_from_slice(&buf[..n]);
    Ok(n)
  }
}

fn tls_config() -> Arc<rustls::ClientConfig> {
  let roots = rustls::RootCertStore {
    roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
  };
  Arc::new(
    rustls::ClientConfig::builder()
      .with_root_certificates(roots)
      .with_no_client_auth(),
  )
}

async fn connect(ep: &Endpoint) -> Result<Stream> {
  let stream = TcpStream::connect((ep.host.as_str(), ep.port))
    .await
    .map_err(|_| KvError::IoFail)?;
  if !ep.tls {
    return Ok(Stream::Plain(stream));
  }
  let connector = compio_tls::TlsConnector::from(tls_config());
  let tls = connector
    .connect(&ep.sni, stream)
    .await
    .map_err(|_| KvError::IoFail)?;
  Ok(Stream::Tls(Box::new(tls)))
}

/// 发起一次请求。`path_and_query` 必须是已经编码好的请求行目标。
pub async fn request(
  ep: &Endpoint,
  method: &str,
  path_and_query: &str,
  header_li: &[(String, String)],
  body: &[u8],
) -> Result<Response> {
  let mut stream = connect(ep).await?;
  let mut head = format!("{method} {path_and_query} HTTP/1.1\r\n");
  for (name, value) in header_li {
    head.push_str(name);
    head.push_str(": ");
    head.push_str(value);
    head.push_str("\r\n");
  }
  head.push_str(&format!("Content-Length: {}\r\n", body.len()));
  head.push_str("Connection: close\r\n\r\n");
  stream.write_all_bytes(head.as_bytes()).await?;
  if !body.is_empty() {
    stream.write_all_bytes(body).await?;
  }

  // 先把响应头读全，再按 Content-Length / chunked 收 body。
  let mut buf: Vec<u8> = Vec::new();
  let mut head_end = None;
  while head_end.is_none() {
    let n = stream.read_into(&mut buf, 16 * 1024).await?;
    if n == 0 {
      return Err(KvError::IoFail);
    }
    head_end = find_head_end(&buf);
  }
  let head_end = head_end.expect("head_end 已确认存在");
  let mut body_buf = buf.split_off(head_end);
  let head = String::from_utf8_lossy(&buf[..head_end - 4]).into_owned();
  let mut lines = head.split("\r\n");
  let status_line = lines.next().ok_or(KvError::IoFail)?;
  let status = status_line
    .split_whitespace()
    .nth(1)
    .and_then(|code| code.parse::<u16>().ok())
    .ok_or(KvError::IoFail)?;
  let mut header_li = Vec::new();
  for line in lines {
    if let Some((name, value)) = line.split_once(':') {
      header_li.push((name.trim().to_owned(), value.trim().to_owned()));
    }
  }

  let response = Response {
    status,
    header_li,
    body: Bytes::new(),
  };
  let chunked = response
    .header("transfer-encoding")
    .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));
  let content_len = response
    .header("content-length")
    .and_then(|value| value.parse::<usize>().ok());

  let body = if chunked {
    loop {
      if let Some((body, consumed)) = decode_chunked(&body_buf) {
        let _ = consumed;
        break body;
      }
      if stream.read_into(&mut body_buf, 64 * 1024).await? == 0 {
        return Err(KvError::IoFail);
      }
    }
  } else {
    let want = content_len.unwrap_or(0);
    while body_buf.len() < want {
      if stream.read_into(&mut body_buf, 256 * 1024).await? == 0 {
        return Err(KvError::IoFail);
      }
    }
    body_buf.truncate(want);
    Bytes::from(body_buf)
  };

  Ok(Response {
    status,
    header_li: response.header_li,
    body,
  })
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
  buf
    .windows(4)
    .position(|window| window == b"\r\n\r\n")
    .map(|pos| pos + 4)
}

/// 解析 chunked body；未收全返回 `None`。
fn decode_chunked(buf: &[u8]) -> Option<(Bytes, usize)> {
  let mut out = Vec::new();
  let mut pos = 0usize;
  loop {
    let line_end = buf[pos..].windows(2).position(|w| w == b"\r\n")? + pos;
    let size_str = std::str::from_utf8(&buf[pos..line_end]).ok()?;
    let size = usize::from_str_radix(size_str.split(';').next()?.trim(), 16).ok()?;
    let data_start = line_end + 2;
    if size == 0 {
      // 结尾：可能需要读 trailer，这里按 `\r\n` 收尾处理。
      return Some((Bytes::from(out), buf.len()));
    }
    if buf.len() < data_start + size + 2 {
      return None;
    }
    out.extend_from_slice(&buf[data_start..data_start + size]);
    pos = data_start + size + 2;
  }
}
