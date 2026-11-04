use std::time::{SystemTime, UNIX_EPOCH};

use motedb::types::Value;

pub fn now_secs() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}

pub fn text_of(value: &Value) -> Option<&str> {
  match value {
    Value::Text(s) => Some(s.as_str()),
    _ => None,
  }
}

pub fn cell_text(value: &Value) -> String {
  match value {
    Value::Integer(i) => i.to_string(),
    Value::Text(s) => s.to_string(),
    Value::Bool(b) => b.to_string(),
    _ => format!("{value:?}"),
  }
}
