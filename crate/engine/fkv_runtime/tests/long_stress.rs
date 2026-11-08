use std::{
  env::var,
  fs::read_dir,
  path::{Path, PathBuf},
  sync::Arc,
  thread,
  time::Duration,
};

use fkv_btree::BatchEntry;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const SHARD_NUM: u16 = 4;
const TABLE_NUM: usize = 8;
const BASE_NUM: usize = 4;
const BATCH_NUM: usize = 5;
const BATCH_SIZE: usize = 10;
const READER_NUM: usize = 2;
const READ_ITER: usize = 20;
const KEYS_PER_TABLE_PER_ROUND: usize = BATCH_NUM * BATCH_SIZE;

fn config(root: PathBuf) -> RuntimeConfig {
  let mut config = RuntimeConfig::for_tests(root);
  config.num_threads = SHARD_NUM;
  config.queue_size = 256;
  config
}

fn table_li() -> Vec<TableIdent> {
  let mut table_li = Vec::new();
  let mut per_shard = vec![0usize; usize::from(SHARD_NUM)];
  let mut index = 0usize;
  while table_li.len() < TABLE_NUM {
    // 路由按 partition_id 取模（对齐 C++），故用不同分区号分散到各 shard。
    let table = TableIdent::new(format!("stress{index}"), index as u32 + 1).unwrap();
    let shard = usize::from(table.shard_index(SHARD_NUM));
    if per_shard[shard] < TABLE_NUM / usize::from(SHARD_NUM) {
      per_shard[shard] += 1;
      table_li.push(table);
    }
    index += 1;
  }
  table_li
}

fn base_key(index: usize) -> Vec<u8> {
  format!("b:{index:04}").into_bytes()
}

fn base_value(table_index: usize, index: usize) -> Vec<u8> {
  format!("base:{table_index}:{index}").into_bytes()
}

fn round_key(round: usize, worker: usize, index: usize) -> Vec<u8> {
  format!("w:{round:04}:{worker}:{index:04}").into_bytes()
}

fn round_value(round: usize, worker: usize, index: usize) -> Vec<u8> {
  format!("value:{round}:{worker}:{index}").into_bytes()
}

fn fd_count() -> Option<usize> {
  let dir = if PathBuf::from("/proc/self/fd").exists() {
    PathBuf::from("/proc/self/fd")
  } else {
    PathBuf::from("/dev/fd")
  };
  read_dir(dir).ok().map(|entry_li| entry_li.count())
}

#[cfg(target_os = "linux")]
fn thread_count() -> Option<usize> {
  read_dir("/proc/self/task")
    .ok()
    .map(|entry_li| entry_li.count())
}

#[cfg(not(target_os = "linux"))]
fn thread_count() -> Option<usize> {
  None
}

fn open_runtime(root: &Path, term: u64) -> Runtime {
  let runtime = Runtime::start(config(root.to_path_buf())).unwrap();
  pollster::block_on(async {
    for shard_id in 0..u32::from(SHARD_NUM) {
      runtime.attach_shard(shard_id, "main", term).await.unwrap();
    }
  });
  runtime
}

fn run_stress(rounds: usize) {
  let root = TempRoot::new("long_stress");
  let table_li = table_li();
  let fd_baseline = fd_count().unwrap_or(0);
  let thread_baseline = thread_count().unwrap_or(0);
  let mut fd_max = fd_baseline;

  for round in 0..rounds {
    let runtime = Arc::new(open_runtime(root.path(), (round + 1) as u64));
    let mut writer_li = Vec::new();
    let mut reader_li = Vec::new();

    if round == 0 {
      pollster::block_on(async {
        for (table_index, tbl) in table_li.iter().enumerate() {
          let batch: Vec<BatchEntry> = (0..BASE_NUM)
            .map(|index| BatchEntry::upsert(base_key(index), base_value(table_index, index), 1))
            .collect();
          runtime.put(tbl, batch).await.unwrap();
        }
      });
    }

    for worker in 0..4usize {
      let runtime = runtime.clone();
      let table_li = table_li.clone();
      writer_li.push(thread::spawn(move || {
        pollster::block_on(async {
          for table_offset in [worker, worker + 4] {
            let tbl = &table_li[table_offset];
            for batch_index in 0..BATCH_NUM {
              let batch: Vec<BatchEntry> = (0..BATCH_SIZE)
                .map(|index| {
                  let key_index = batch_index * BATCH_SIZE + index;
                  BatchEntry::upsert(
                    round_key(round, worker, key_index),
                    round_value(round, worker, key_index),
                    2,
                  )
                })
                .collect();
              runtime.put(tbl, batch).await.unwrap();
            }
          }
        });
      }));
    }

    for reader in 0..READER_NUM {
      let runtime = runtime.clone();
      let table_li = table_li.clone();
      reader_li.push(thread::spawn(move || {
        pollster::block_on(async {
          for iteration in 0..READ_ITER {
            let table_index = (iteration + reader) % table_li.len();
            let tbl = &table_li[table_index];
            for index in 0..BASE_NUM {
              assert_eq!(
                runtime.get(tbl, &base_key(index)).await.unwrap(),
                Some(base_value(table_index, index))
              );
            }
            let entry_li = runtime.scan(tbl, b"b:", b"c:", 0).await.unwrap();
            assert_eq!(entry_li.len(), BASE_NUM);
          }
        });
      }));
    }

    for handle in writer_li {
      handle.join().unwrap();
    }
    for handle in reader_li {
      handle.join().unwrap();
    }

    // reopen + 全量校验：每个 table 有 base + 每轮 50 个 key。
    pollster::block_on(async {
      for (table_index, tbl) in table_li.iter().enumerate() {
        runtime.reopen(tbl).await.unwrap();
        for index in 0..BASE_NUM {
          assert_eq!(
            runtime.get(tbl, &base_key(index)).await.unwrap(),
            Some(base_value(table_index, index))
          );
        }
        let entry_li = runtime.scan(tbl, b"", b"", 0).await.unwrap();
        assert_eq!(
          entry_li.len(),
          BASE_NUM + (round + 1) * KEYS_PER_TABLE_PER_ROUND
        );
        let worker = table_index % 4;
        for key_index in 0..KEYS_PER_TABLE_PER_ROUND {
          assert_eq!(
            runtime
              .get(tbl, &round_key(round, worker, key_index))
              .await
              .unwrap(),
            Some(round_value(round, worker, key_index))
          );
        }
      }
    });

    let runtime = Arc::try_unwrap(runtime).ok().expect("runtime still shared");
    pollster::block_on(runtime.shutdown()).unwrap();
    thread::sleep(Duration::from_millis(30));
    if let Some(fd_count) = fd_count() {
      fd_max = fd_max.max(fd_count);
    }
    if let Some(thread_count) = thread_count() {
      assert!(
        thread_count <= thread_baseline + 2,
        "thread leak after round {round}: baseline={thread_baseline} now={thread_count}"
      );
    }
  }

  assert!(
    fd_max <= fd_baseline + 16,
    "fd leak: baseline={fd_baseline} max={fd_max}"
  );
}

#[test]
fn multi_round_concurrent_stress_with_reopen() {
  run_stress(4);
}

/// 长稳模式：`cargo test -p fkv_runtime --test long_stress -- --ignored --nocapture`
#[test]
#[ignore = "long-running stress"]
fn long_running_stress() {
  let rounds = var("FKV_STRESS_ROUNDS")
    .ok()
    .and_then(|value| value.parse().ok())
    .unwrap_or(100);
  run_stress(rounds);
}
