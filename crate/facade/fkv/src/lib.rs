//! fkv curated facade：只导出稳定入口，内部 crate 仍可单独使用。

pub use fkv_config::KvOptions;
pub use fkv_error::{KvError, Result};
pub use fkv_sdk::{KvApi, KvStore, now_ts};
pub use fkv_store::ScanOptions;
pub use fkv_types::TableIdent;
