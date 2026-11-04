use std::{io, result};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
  #[error(transparent)]
  Io(#[from] io::Error),
  #[error(transparent)]
  Mote(#[from] motedb::MoteDBError),
  #[error("tree-sitter: {0}")]
  TreeSitter(String),
  #[error("invalid argument: {0}")]
  Invalid(String),
}

pub type Result<T> = result::Result<T, LedgerError>;
