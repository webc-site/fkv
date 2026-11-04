//! S3 backend：`object_store::aws` + 独立 tokio 线程。
//!
//! shard 线程只发命令 + 等 oneshot，不直接跑 tokio/http。

use std::{
  result,
  thread::{self, JoinHandle},
};

use crossfire::{
  MAsyncTx,
  mpmc::{Array, bounded_async},
};
use fkv_error::{KvError, Result};
use fkv_object::{ObjectMeta, ObjectStore};
use futures::{StreamExt, channel::oneshot, executor::block_on};
use object_store::{
  Error, ObjectStore as _, ObjectStoreExt, PutMode, UpdateVersion,
  aws::{AmazonS3, AmazonS3Builder},
  path::Path as OsPath,
};
use tokio::runtime::Builder;

#[derive(Debug, Clone)]
pub struct S3Config {
  pub bucket: String,
  pub endpoint: String,
  pub region: String,
  pub access_key: String,
  pub secret_key: String,
  pub allow_http: bool,
  pub prefix: String,
}

struct Command {
  kind: CommandKind,
  reply: oneshot::Sender<Result<CommandReply>>,
}

enum CommandKind {
  Get(String),
  Head(String),
  Put {
    key: String,
    data: Vec<u8>,
    if_match: Option<String>,
    if_none_match: bool,
  },
  Delete(String),
  List(String),
  Shutdown,
}

enum CommandReply {
  Get(Option<Vec<u8>>),
  Head(Option<ObjectMeta>),
  Put(String),
  Unit,
  List(Vec<ObjectMeta>),
}

pub struct S3ObjectStore {
  tx: MAsyncTx<Array<Command>>,
  join: Option<JoinHandle<()>>,
}

impl S3ObjectStore {
  pub fn start(config: S3Config) -> Result<Self> {
    let (tx, rx) = bounded_async::<Command>(64);
    let join = thread::Builder::new()
      .name("fkv-s3".to_owned())
      .spawn(move || {
        let Ok(runtime) = Builder::new_current_thread().enable_all().build() else {
          return;
        };
        runtime.block_on(async move {
          let store = build_store(&config);
          while let Ok(command) = rx.recv().await {
            let shutdown = matches!(command.kind, CommandKind::Shutdown);
            let reply = match &store {
              Ok(store) => handle(store, command.kind).await,
              Err(err) => Err(*err),
            };
            let _ = command.reply.send(reply);
            if shutdown {
              break;
            }
          }
        });
      })
      .map_err(|_| KvError::IoFail)?;
    Ok(Self {
      tx,
      join: Some(join),
    })
  }

  pub fn shutdown(mut self) -> Result<()> {
    let _ = block_on(self.send(CommandKind::Shutdown));
    if let Some(join) = self.join.take() {
      join.join().map_err(|_| KvError::IoFail)?;
    }
    Ok(())
  }
}

fn build_store(config: &S3Config) -> result::Result<AmazonS3, KvError> {
  let mut builder = AmazonS3Builder::new()
    .with_bucket_name(&config.bucket)
    .with_region(&config.region)
    .with_access_key_id(&config.access_key)
    .with_secret_access_key(&config.secret_key)
    .with_allow_http(config.allow_http)
    .with_virtual_hosted_style_request(false);
  if !config.endpoint.is_empty() {
    builder = builder.with_endpoint(&config.endpoint);
  }
  builder.build().map_err(|_| KvError::CloudErr)
}

async fn handle(store: &AmazonS3, kind: CommandKind) -> Result<CommandReply> {
  match kind {
    CommandKind::Head(key) => match store.head(&OsPath::from(key.as_str())).await {
      Ok(meta) => Ok(CommandReply::Head(Some(ObjectMeta {
        key: meta.location.to_string(),
        etag: meta.e_tag.unwrap_or_default(),
        size: meta.size,
      }))),
      Err(Error::NotFound { .. }) => Ok(CommandReply::Head(None)),
      Err(_) => Err(KvError::CloudErr),
    },
    CommandKind::Get(key) => match store.get(&OsPath::from(key.as_str())).await {
      Ok(result) => match result.bytes().await {
        Ok(bytes) => Ok(CommandReply::Get(Some(bytes.to_vec()))),
        Err(_) => Err(KvError::CloudErr),
      },
      Err(Error::NotFound { .. }) => Ok(CommandReply::Get(None)),
      Err(_) => Err(KvError::CloudErr),
    },
    CommandKind::Put {
      key,
      data,
      if_match,
      if_none_match,
    } => {
      let mode = if if_none_match {
        PutMode::Create
      } else if let Some(etag) = if_match {
        PutMode::Update(UpdateVersion {
          e_tag: Some(etag),
          version: None,
        })
      } else {
        PutMode::Overwrite
      };
      match store
        .put_opts(&OsPath::from(key.as_str()), data.into(), mode.into())
        .await
      {
        Ok(result) => Ok(CommandReply::Put(result.e_tag.unwrap_or_default())),
        Err(Error::AlreadyExists { .. }) => Err(KvError::AlreadyExists),
        Err(Error::Precondition { .. }) => Err(KvError::Busy),
        Err(_) => Err(KvError::CloudErr),
      }
    }
    CommandKind::Delete(key) => match store.delete(&OsPath::from(key.as_str())).await {
      Ok(()) => Ok(CommandReply::Unit),
      Err(Error::NotFound { .. }) => Ok(CommandReply::Unit),
      Err(_) => Err(KvError::CloudErr),
    },
    CommandKind::Shutdown => Ok(CommandReply::Unit),
    CommandKind::List(prefix) => {
      let mut meta_li = Vec::new();
      let mut stream = store.list(Some(&OsPath::from(prefix.as_str())));
      while let Some(item) = stream.next().await {
        match item {
          Ok(meta) => meta_li.push(ObjectMeta {
            key: meta.location.to_string(),
            etag: meta.e_tag.unwrap_or_default(),
            size: meta.size,
          }),
          Err(_) => return Err(KvError::CloudErr),
        }
      }
      Ok(CommandReply::List(meta_li))
    }
  }
}

impl ObjectStore for S3ObjectStore {
  async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
    match self.send(CommandKind::Get(key.to_owned())).await? {
      CommandReply::Get(value) => Ok(value),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn put(
    &self,
    key: &str,
    value: &[u8],
    if_match: Option<&str>,
    if_none_match: bool,
  ) -> Result<String> {
    match self
      .send(CommandKind::Put {
        key: key.to_owned(),
        data: value.to_vec(),
        if_match: if_match.map(str::to_owned),
        if_none_match,
      })
      .await?
    {
      CommandReply::Put(etag) => Ok(etag),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn head(&self, key: &str) -> Result<Option<ObjectMeta>> {
    match self.send(CommandKind::Head(key.to_owned())).await? {
      CommandReply::Head(meta) => Ok(meta),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn delete(&self, key: &str) -> Result<()> {
    match self.send(CommandKind::Delete(key.to_owned())).await? {
      CommandReply::Unit => Ok(()),
      _ => Err(KvError::CloudErr),
    }
  }

  async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>> {
    match self.send(CommandKind::List(prefix.to_owned())).await? {
      CommandReply::List(meta_li) => Ok(meta_li),
      _ => Err(KvError::CloudErr),
    }
  }
}

impl S3ObjectStore {
  async fn send(&self, kind: CommandKind) -> Result<CommandReply> {
    let (reply, rx) = oneshot::channel();
    self
      .tx
      .send(Command { kind, reply })
      .await
      .map_err(|_| KvError::NotRunning)?;
    rx.await.map_err(|_| KvError::NotRunning)?
  }
}
