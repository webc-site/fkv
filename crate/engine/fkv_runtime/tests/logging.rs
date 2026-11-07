use std::sync::{Arc, Mutex, Once, OnceLock};

use fkv_btree::BatchEntry;
use fkv_error::KvError;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;
use log::{LevelFilter, Log, Metadata, Record};

struct BufferLogger {
  buf: Arc<Mutex<Vec<String>>>,
}

impl Log for BufferLogger {
  fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
    true
  }

  fn log(&self, record: &Record<'_>) {
    self.buf.lock().unwrap().push(format!(
      "{} {} {}",
      record.level(),
      record.target(),
      record.args()
    ));
  }

  fn flush(&self) {}
}

fn install_logger() -> Arc<Mutex<Vec<String>>> {
  static ONCE: Once = Once::new();
  static BUF: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
  ONCE.call_once(|| {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let _ = BUF.set(buf.clone());
    let _ = log::set_boxed_logger(Box::new(BufferLogger { buf }));
    log::set_max_level(LevelFilter::Debug);
  });
  BUF.get().unwrap().clone()
}

#[test]
fn runtime_and_shard_emit_structured_logs() {
  let buf = install_logger();
  buf.lock().unwrap().clear();
  let root = TempRoot::new("logging");
  pollster::block_on(async {
    let runtime = Runtime::start(RuntimeConfig {
      num_threads: 1,
      root: root.path().to_path_buf(),
      page_size: 512,
      restart_interval: 16,
      pages_per_file_shift: 18,
      segment_size: 4096,
      segments_per_file_shift: 2,
      large_value_threshold: 1024,
      fixed_buffer_size: 0,
      auto_reopen_retry_times: 0,
      auto_reopen_pending_time_us: 0,
      archive_interval_secs: 0,
      archive_num_retained: 100,
      queue_size: 64,
    })
    .unwrap();
    runtime.attach_shard(0, "main", 1).await.unwrap();
    let tbl = TableIdent::new("demo", 1).unwrap();
    runtime
      .put(
        &tbl,
        vec![BatchEntry::upsert(b"a".to_vec(), b"A".to_vec(), 1)],
      )
      .await
      .unwrap();
    assert_eq!(
      runtime
        .put_with_term(
          &tbl,
          vec![BatchEntry::upsert(b"a".to_vec(), b"A2".to_vec(), 2)],
          2,
        )
        .await
        .unwrap_err(),
      KvError::ExpiredTerm
    );
    runtime.shutdown().await.unwrap();
  });

  let joined = buf.lock().unwrap().join("\n");
  assert!(joined.contains("runtime start"), "{joined}");
  assert!(joined.contains("threads=1"), "{joined}");
  assert!(
    joined.contains(&format!("root={}", root.path().display())),
    "{joined}"
  );
  assert!(joined.contains("shard start shard_id=0"), "{joined}");
  assert!(
    joined.contains("shard attach shard_id=0 branch=main term=1"),
    "{joined}"
  );
  assert!(joined.contains("op=put status=ok"), "{joined}");
  assert!(
    joined.contains("op=put status=error error=Expired term"),
    "{joined}"
  );
  assert!(joined.contains("shard stop shard_id=0"), "{joined}");
  assert!(joined.contains("runtime shutdown complete"), "{joined}");
}
