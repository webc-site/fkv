//! 行式文本 op log：一行一个 op，参数与 payload 用 hex 编码。
//!
//! 两层格式：
//!
//! - [`OpRecord`] / [`Op`]：`SimStore` 的底层 IO op log。只回放写类操作
//!   （`clock=1 delay=0 op=write_page tbl=64656d6f@1 fp=0 len=8 payload=...`）；
//!   读 / 列举 / 注册缓冲等纯观察型变体在序列化与解析时显式报
//!   [`OpLogError::UnsupportedOp`]，绝不静默丢弃。
//! - [`EngineOp`]：引擎级操作（put / delete / truncate / archive / branch /
//!   reopen），供崩溃回放与确定性 dump 测试共用。
//!
//! 文本本身不含空白分隔的歧义：表名走 `hex(name)@partition`，二进制
//! （key / value / payload）统一小写 hex。

use core::fmt;
use std::error::Error;

use fkv_types::TableIdent;

use crate::{Op, OpRecord};

/// op log 文本格式错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpLogError {
  /// 未知 op 名。
  UnknownOp { line: usize, op: String },
  /// 该底层 IO 变体无法进入可回放文本格式。
  UnsupportedOp { line: usize, op: String },
  /// 字段缺失 / 重复 / 非法。
  Syntax { line: usize, message: String },
}

impl fmt::Display for OpLogError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::UnknownOp { line, op } => write!(f, "line {line}: 未知 op `{op}`"),
      Self::UnsupportedOp { line, op } => {
        write!(f, "line {line}: 底层 IO 变体 `{op}` 不支持文本回放")
      }
      Self::Syntax { line, message } => write!(f, "line {line}: {message}"),
    }
  }
}

impl Error for OpLogError {}

fn syntax(line: usize, message: impl Into<String>) -> OpLogError {
  OpLogError::Syntax {
    line,
    message: message.into(),
  }
}

fn hex_digit(byte: u8) -> u8 {
  if byte < 10 {
    b'0' + byte
  } else {
    b'a' + byte - 10
  }
}

/// 小写 hex 编码。
pub fn hex_encode(bytes: &[u8]) -> String {
  let mut out = String::with_capacity(bytes.len() * 2);
  for byte in bytes {
    out.push(hex_digit(byte >> 4) as char);
    out.push(hex_digit(byte & 0x0F) as char);
  }
  out
}

fn hex_value(byte: u8) -> Option<u8> {
  match byte {
    b'0'..=b'9' => Some(byte - b'0'),
    b'a'..=b'f' => Some(byte - b'a' + 10),
    b'A'..=b'F' => Some(byte - b'A' + 10),
    _ => None,
  }
}

/// hex 解码；奇数长度或非法字符返回 `None`。
pub fn hex_decode(text: &str) -> Option<Vec<u8>> {
  if !text.len().is_multiple_of(2) {
    return None;
  }
  text
    .as_bytes()
    .as_chunks::<2>()
    .0
    .iter()
    .map(|pair| Some(hex_value(pair[0])? << 4 | hex_value(pair[1])?))
    .collect()
}

fn tbl_token(tbl: &TableIdent) -> String {
  format!(
    "{}@{}",
    hex_encode(tbl.tbl_name.as_bytes()),
    tbl.partition_id
  )
}

fn parse_tbl(line: usize, field: &str, text: &str) -> Result<TableIdent, OpLogError> {
  let Some((name_hex, partition)) = text.rsplit_once('@') else {
    return Err(syntax(line, format!("{field} 需要 hex(name)@partition")));
  };
  let name = hex_decode(name_hex)
    .and_then(|bytes| String::from_utf8(bytes).ok())
    .ok_or_else(|| syntax(line, format!("{field} 表名不是合法 hex UTF-8")))?;
  let partition = partition
    .parse::<u32>()
    .map_err(|_| syntax(line, format!("{field} 分区号非法")))?;
  TableIdent::new(name, partition).map_err(|_| syntax(line, format!("{field} 表标识非法")))
}

fn parse_u64(line: usize, field: &str, text: &str) -> Result<u64, OpLogError> {
  text
    .parse::<u64>()
    .map_err(|_| syntax(line, format!("{field} 不是 u64: {text}")))
}

fn parse_u32(line: usize, field: &str, text: &str) -> Result<u32, OpLogError> {
  text
    .parse::<u32>()
    .map_err(|_| syntax(line, format!("{field} 不是 u32: {text}")))
}

fn parse_usize(line: usize, field: &str, text: &str) -> Result<usize, OpLogError> {
  text
    .parse::<usize>()
    .map_err(|_| syntax(line, format!("{field} 不是 usize: {text}")))
}

fn parse_hex(line: usize, field: &str, text: &str) -> Result<Vec<u8>, OpLogError> {
  hex_decode(text).ok_or_else(|| syntax(line, format!("{field} 不是合法 hex")))
}

/// 一行内的 `key=value` 集合；重复字段直接报错，避免静默取首个。
struct Fields<'a> {
  line: usize,
  item_li: Vec<(&'a str, &'a str)>,
}

impl<'a> Fields<'a> {
  fn parse(line: usize, text: &'a str) -> Result<Self, OpLogError> {
    let mut item_li = Vec::new();
    for item in text.split_whitespace() {
      let Some((key, value)) = item.split_once('=') else {
        return Err(syntax(line, format!("字段 `{item}` 缺少 '='")));
      };
      if item_li.iter().any(|(existing, _)| *existing == key) {
        return Err(syntax(line, format!("字段 `{key}` 重复")));
      }
      item_li.push((key, value));
    }
    Ok(Self { line, item_li })
  }

  fn take(&mut self, key: &'static str) -> Option<&'a str> {
    let index = self.item_li.iter().position(|(name, _)| *name == key)?;
    Some(self.item_li.remove(index).1)
  }

  fn need(&mut self, key: &'static str) -> Result<&'a str, OpLogError> {
    self
      .take(key)
      .ok_or_else(|| syntax(self.line, format!("缺少字段 `{key}`")))
  }

  fn finish(self) -> Result<(), OpLogError> {
    match self.item_li.first() {
      Some((key, _)) => Err(syntax(self.line, format!("未知字段 `{key}`"))),
      None => Ok(()),
    }
  }
}

impl Op {
  /// 写类 / 控制类变体可进入文本格式；读、列举、注册缓冲等观察型变体返回 false。
  fn replayable(&self) -> bool {
    matches!(
      self,
      Self::WritePage { .. }
        | Self::SyncData { .. }
        | Self::FlushPage { .. }
        | Self::ResetManifest { .. }
        | Self::ResetManifestFile { .. }
        | Self::AppendManifest { .. }
        | Self::AppendManifestFile { .. }
        | Self::SwitchManifest { .. }
        | Self::DropManifest { .. }
        | Self::WriteManifestFile { .. }
        | Self::RemoveManifestFile { .. }
        | Self::RemoveSidecarFileName { .. }
        | Self::UpsertTerm { .. }
        | Self::WriteSegment { .. }
        | Self::FlushSegment { .. }
        | Self::WriteSegmentFixed { .. }
        | Self::WriteFile { .. }
        | Self::RemoveFile { .. }
        | Self::RemoveSegmentFile { .. }
        | Self::Crash
    )
  }

  /// 序列化 op 参数（不含 clock / delay / payload）。
  fn param_text(&self) -> String {
    let mut out = String::new();
    match self {
      Self::WritePage {
        tbl,
        file_page_id,
        len,
      } => {
        push_field(&mut out, "tbl", &tbl_token(tbl))
          .field("fp", *file_page_id)
          .field("len", *len);
      }
      Self::SyncData { tbl } => {
        push_field(&mut out, "tbl", &tbl_token(tbl));
      }
      Self::FlushPage { tbl, file_page_id } => {
        push_field(&mut out, "tbl", &tbl_token(tbl)).field("fp", *file_page_id);
      }
      Self::ResetManifest { tbl, size } => {
        push_field(&mut out, "tbl", &tbl_token(tbl)).field("size", *size);
      }
      Self::ResetManifestFile { tbl, name, size } => {
        push_field(&mut out, "tbl", &tbl_token(tbl))
          .field("name", name)
          .field("size", *size);
      }
      Self::AppendManifest { tbl, len } => {
        push_field(&mut out, "tbl", &tbl_token(tbl)).field("len", *len);
      }
      Self::AppendManifestFile { tbl, name, len } => {
        push_field(&mut out, "tbl", &tbl_token(tbl))
          .field("name", name)
          .field("len", *len);
      }
      Self::SwitchManifest { tbl, len } => {
        push_field(&mut out, "tbl", &tbl_token(tbl)).field("len", *len);
      }
      Self::DropManifest { tbl } => {
        push_field(&mut out, "tbl", &tbl_token(tbl));
      }
      Self::WriteManifestFile { tbl, name, len } => {
        push_field(&mut out, "tbl", &tbl_token(tbl))
          .field("name", name)
          .field("len", *len);
      }
      Self::RemoveManifestFile { tbl, name } | Self::RemoveSidecarFileName { tbl, name } => {
        push_field(&mut out, "tbl", &tbl_token(tbl)).field("name", name);
      }
      Self::UpsertTerm {
        branch,
        shard_id,
        term,
      } => {
        push_field(&mut out, "branch", branch)
          .field("shard", *shard_id)
          .field("term", *term);
      }
      Self::WriteSegment {
        tbl,
        segment_fp_id,
        len,
      } => {
        push_field(&mut out, "tbl", &tbl_token(tbl))
          .field("fp", *segment_fp_id)
          .field("len", *len);
      }
      Self::FlushSegment { tbl, segment_fp_id } => {
        push_field(&mut out, "tbl", &tbl_token(tbl)).field("fp", *segment_fp_id);
      }
      Self::WriteSegmentFixed {
        tbl,
        segment_fp_id,
        offset,
        len,
      } => {
        push_field(&mut out, "tbl", &tbl_token(tbl))
          .field("fp", *segment_fp_id)
          .field("offset", *offset)
          .field("len", *len);
      }
      Self::WriteFile { tbl, file_id, len } => {
        push_field(&mut out, "tbl", &tbl_token(tbl))
          .field("file", *file_id)
          .field("len", *len);
      }
      Self::RemoveFile { tbl, file_id } | Self::RemoveSegmentFile { tbl, file_id } => {
        push_field(&mut out, "tbl", &tbl_token(tbl)).field("file", *file_id);
      }
      // 纯观察型变体不产生参数，序列化入口会先报 UnsupportedOp。
      Self::ReadPage { .. }
      | Self::ReadManifest { .. }
      | Self::ListManifestFileNameLi { .. }
      | Self::ListSidecarFileNameLi { .. }
      | Self::ReadManifestFile { .. }
      | Self::ReadTerm { .. }
      | Self::ReadSegment { .. }
      | Self::ReadSegmentFixed { .. }
      | Self::RegisterFixedBuffer { .. }
      | Self::ListFileIdLi { .. }
      | Self::ReadFile { .. }
      | Self::FileSize { .. }
      | Self::ListSegmentFileIdLi { .. }
      | Self::Crash => {}
    }
    out
  }

  /// 生成一行 `op=...` 文本；不支持的观察型变体显式报错。
  pub fn to_text(&self, line: usize) -> Result<String, OpLogError> {
    if !self.replayable() {
      return Err(OpLogError::UnsupportedOp {
        line,
        op: self.kind_name().to_owned(),
      });
    }
    let name = self.kind_name();
    let params = self.param_text();
    Ok(if params.is_empty() {
      format!("op={name}")
    } else {
      format!("op={name} {params}")
    })
  }

  fn kind_name(&self) -> &'static str {
    match self {
      Self::ReadPage { .. } => "read_page",
      Self::WritePage { .. } => "write_page",
      Self::SyncData { .. } => "sync_data",
      Self::FlushPage { .. } => "flush_page",
      Self::ResetManifest { .. } => "reset_manifest",
      Self::ResetManifestFile { .. } => "reset_manifest_file",
      Self::AppendManifest { .. } => "append_manifest",
      Self::AppendManifestFile { .. } => "append_manifest_file",
      Self::SwitchManifest { .. } => "switch_manifest",
      Self::ReadManifest { .. } => "read_manifest",
      Self::DropManifest { .. } => "drop_manifest",
      Self::WriteManifestFile { .. } => "write_manifest_file",
      Self::ListManifestFileNameLi { .. } => "list_manifest_file_name_li",
      Self::ListSidecarFileNameLi { .. } => "list_sidecar_file_name_li",
      Self::ReadManifestFile { .. } => "read_manifest_file",
      Self::RemoveManifestFile { .. } => "remove_manifest_file",
      Self::RemoveSidecarFileName { .. } => "remove_sidecar_file_name",
      Self::ReadTerm { .. } => "read_term",
      Self::UpsertTerm { .. } => "upsert_term",
      Self::ReadSegment { .. } => "read_segment",
      Self::WriteSegment { .. } => "write_segment",
      Self::FlushSegment { .. } => "flush_segment",
      Self::ReadSegmentFixed { .. } => "read_segment_fixed",
      Self::WriteSegmentFixed { .. } => "write_segment_fixed",
      Self::RegisterFixedBuffer { .. } => "register_fixed_buffer",
      Self::ListFileIdLi { .. } => "list_file_id_li",
      Self::ReadFile { .. } => "read_file",
      Self::WriteFile { .. } => "write_file",
      Self::FileSize { .. } => "file_size",
      Self::RemoveFile { .. } => "remove_file",
      Self::ListSegmentFileIdLi { .. } => "list_segment_file_id_li",
      Self::RemoveSegmentFile { .. } => "remove_segment_file",
      Self::Crash => "crash",
    }
  }
}

/// 参数拼装小工具，避免在每个分支重复 `push_str` 样板。
struct ParamText<'a> {
  out: &'a mut String,
}

fn push_field<'a>(out: &'a mut String, key: &str, value: &str) -> ParamText<'a> {
  if !out.is_empty() {
    out.push(' ');
  }
  out.push_str(key);
  out.push('=');
  out.push_str(value);
  ParamText { out }
}

impl ParamText<'_> {
  fn field(self, key: &str, value: impl fmt::Display) -> Self {
    let ParamText { out } = self;
    push_field(out, key, &value.to_string())
  }
}

impl OpRecord {
  /// 一行文本：`clock=.. delay=.. op=.. <参数> [payload=..]`。
  pub fn to_text(&self, line: usize) -> Result<String, OpLogError> {
    let op_text = self.op.to_text(line)?;
    let mut out = format!("clock={} delay={} {op_text}", self.clock, self.delay);
    if let Some(payload) = &self.payload {
      out.push_str(" payload=");
      out.push_str(&hex_encode(payload));
    }
    Ok(out)
  }
}

/// 解析单条底层 op 记录（含 clock / delay / payload）。
pub fn parse_record(line_no: usize, text: &str) -> Result<OpRecord, OpLogError> {
  let mut fields = Fields::parse(line_no, text)?;
  let clock = parse_u64(line_no, "clock", fields.need("clock")?)?;
  let delay = parse_u64(line_no, "delay", fields.need("delay")?)?;
  let op_name = fields.need("op")?;
  let payload = fields
    .take("payload")
    .map(|text| parse_hex(line_no, "payload", text))
    .transpose()?;
  let op = parse_op(line_no, op_name, &mut fields)?;
  fields.finish()?;
  Ok(OpRecord {
    op,
    clock,
    delay,
    payload,
  })
}

fn parse_op(line: usize, name: &str, fields: &mut Fields<'_>) -> Result<Op, OpLogError> {
  let unsupported = || OpLogError::UnsupportedOp {
    line,
    op: name.to_owned(),
  };
  Ok(match name {
    "write_page" => Op::WritePage {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      file_page_id: parse_u64(line, "fp", fields.need("fp")?)?,
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "sync_data" => Op::SyncData {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
    },
    "flush_page" => Op::FlushPage {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      file_page_id: parse_u64(line, "fp", fields.need("fp")?)?,
    },
    "reset_manifest" => Op::ResetManifest {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      size: parse_u64(line, "size", fields.need("size")?)?,
    },
    "reset_manifest_file" => Op::ResetManifestFile {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      name: fields.need("name")?.to_owned(),
      size: parse_u64(line, "size", fields.need("size")?)?,
    },
    "append_manifest" => Op::AppendManifest {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "append_manifest_file" => Op::AppendManifestFile {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      name: fields.need("name")?.to_owned(),
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "switch_manifest" => Op::SwitchManifest {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "drop_manifest" => Op::DropManifest {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
    },
    "write_manifest_file" => Op::WriteManifestFile {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      name: fields.need("name")?.to_owned(),
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "remove_manifest_file" => Op::RemoveManifestFile {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      name: fields.need("name")?.to_owned(),
    },
    "remove_sidecar_file_name" => Op::RemoveSidecarFileName {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      name: fields.need("name")?.to_owned(),
    },
    "upsert_term" => Op::UpsertTerm {
      branch: fields.need("branch")?.to_owned(),
      shard_id: parse_u32(line, "shard", fields.need("shard")?)?,
      term: parse_u64(line, "term", fields.need("term")?)?,
    },
    "write_segment" => Op::WriteSegment {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      segment_fp_id: parse_u64(line, "fp", fields.need("fp")?)?,
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "flush_segment" => Op::FlushSegment {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      segment_fp_id: parse_u64(line, "fp", fields.need("fp")?)?,
    },
    "write_segment_fixed" => Op::WriteSegmentFixed {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      segment_fp_id: parse_u64(line, "fp", fields.need("fp")?)?,
      offset: parse_usize(line, "offset", fields.need("offset")?)?,
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "write_file" => Op::WriteFile {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      file_id: parse_u64(line, "file", fields.need("file")?)?,
      len: parse_usize(line, "len", fields.need("len")?)?,
    },
    "remove_file" => Op::RemoveFile {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      file_id: parse_u64(line, "file", fields.need("file")?)?,
    },
    "remove_segment_file" => Op::RemoveSegmentFile {
      tbl: parse_tbl(line, "tbl", fields.need("tbl")?)?,
      file_id: parse_u64(line, "file", fields.need("file")?)?,
    },
    "crash" => Op::Crash,
    "read_page"
    | "read_manifest"
    | "list_manifest_file_name_li"
    | "list_sidecar_file_name_li"
    | "read_manifest_file"
    | "read_term"
    | "read_segment"
    | "read_segment_fixed"
    | "register_fixed_buffer"
    | "list_file_id_li"
    | "read_file"
    | "file_size"
    | "list_segment_file_id_li" => return Err(unsupported()),
    _ => {
      return Err(OpLogError::UnknownOp {
        line,
        op: name.to_owned(),
      });
    }
  })
}

/// 引擎级操作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOp {
  Put {
    tbl: TableIdent,
    ts: u64,
    key: Vec<u8>,
    value: Vec<u8>,
    expire_ts: u64,
  },
  Delete {
    tbl: TableIdent,
    ts: u64,
    key: Vec<u8>,
  },
  Truncate {
    tbl: TableIdent,
  },
  Archive {
    tbl: TableIdent,
    tag: String,
    keep: u32,
  },
  Branch {
    tbl: TableIdent,
    name: String,
  },
  Reopen {
    tbl: TableIdent,
  },
}

impl EngineOp {
  /// 一行文本，二进制字段为小写 hex。
  pub fn to_text(&self) -> String {
    match self {
      Self::Put {
        tbl,
        ts,
        key,
        value,
        expire_ts,
      } => format!(
        "put tbl={} ts={ts} k={} v={} exp={expire_ts}",
        tbl_token(tbl),
        hex_encode(key),
        hex_encode(value)
      ),
      Self::Delete { tbl, ts, key } => format!(
        "delete tbl={} ts={ts} k={}",
        tbl_token(tbl),
        hex_encode(key)
      ),
      Self::Truncate { tbl } => format!("truncate tbl={}", tbl_token(tbl)),
      Self::Archive { tbl, tag, keep } => {
        format!("archive tbl={} tag={tag} keep={keep}", tbl_token(tbl))
      }
      Self::Branch { tbl, name } => format!("branch tbl={} name={name}", tbl_token(tbl)),
      Self::Reopen { tbl } => format!("reopen tbl={}", tbl_token(tbl)),
    }
  }

  /// 解析一行；未知字段 / 动作显式报错。
  pub fn from_text(line_no: usize, text: &str) -> Result<Self, OpLogError> {
    let Some((name, rest)) = text.split_once(char::is_whitespace) else {
      return Err(syntax(line_no, "缺少 op 参数"));
    };
    let mut fields = Fields::parse(line_no, rest)?;
    let op = match name {
      "put" => {
        let tbl = parse_tbl(line_no, "tbl", fields.need("tbl")?)?;
        let ts = parse_u64(line_no, "ts", fields.need("ts")?)?;
        let key = parse_hex(line_no, "k", fields.need("k")?)?;
        let value = parse_hex(line_no, "v", fields.need("v")?)?;
        let expire_ts = match fields.take("exp") {
          Some(text) => parse_u64(line_no, "exp", text)?,
          None => 0,
        };
        Self::Put {
          tbl,
          ts,
          key,
          value,
          expire_ts,
        }
      }
      "delete" => Self::Delete {
        tbl: parse_tbl(line_no, "tbl", fields.need("tbl")?)?,
        ts: parse_u64(line_no, "ts", fields.need("ts")?)?,
        key: parse_hex(line_no, "k", fields.need("k")?)?,
      },
      "truncate" => Self::Truncate {
        tbl: parse_tbl(line_no, "tbl", fields.need("tbl")?)?,
      },
      "archive" => Self::Archive {
        tbl: parse_tbl(line_no, "tbl", fields.need("tbl")?)?,
        tag: fields.need("tag")?.to_owned(),
        keep: parse_u32(line_no, "keep", fields.need("keep")?)?,
      },
      "branch" => Self::Branch {
        tbl: parse_tbl(line_no, "tbl", fields.need("tbl")?)?,
        name: fields.need("name")?.to_owned(),
      },
      "reopen" => Self::Reopen {
        tbl: parse_tbl(line_no, "tbl", fields.need("tbl")?)?,
      },
      _ => {
        return Err(OpLogError::UnknownOp {
          line: line_no,
          op: name.to_owned(),
        });
      }
    };
    fields.finish()?;
    Ok(op)
  }
}

/// 解析多行引擎 op log；空行与 `#` 注释跳过。
pub fn parse_engine_log(text: &str) -> Result<Vec<EngineOp>, OpLogError> {
  text
    .lines()
    .enumerate()
    .filter_map(|(index, line)| {
      let line = line.trim();
      (!line.is_empty() && !line.starts_with('#')).then_some((index + 1, line))
    })
    .map(|(line_no, line)| EngineOp::from_text(line_no, line))
    .collect()
}

/// 渲染引擎 op log，每条一行，末尾换行。
pub fn render_engine_log(op_li: &[EngineOp]) -> String {
  let mut out = String::new();
  for op in op_li {
    out.push_str(&op.to_text());
    out.push('\n');
  }
  out
}
