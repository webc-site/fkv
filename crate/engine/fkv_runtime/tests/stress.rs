use std::{
  collections::HashMap,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  thread,
};

use fkv_btree::BatchEntry;
use fkv_metrics::Op;
use fkv_runtime::{Runtime, RuntimeConfig};
use fkv_testkit::TempRoot;
use fkv_types::TableIdent;

const TABLE_NUM: usize = 8;
const SHARD_NUM: u16 = 4;
const BASE_NUM: usize = 4;
const BATCH_NUM: usize = 10;
const BATCH_SIZE: usize = 10;
const READER_NUM: usize = 3;
const READ_ITER: usize = 40;

fn tables_per_shard() -> Vec<TableIdent> {
  let mut table_li: Vec<TableIdent> = Vec::new();
  let mut count_per_shard = vec![0usize; usize::from(SHARD_NUM)];
  let mut index = 0usize;
  while table_li.len() < TABLE_NUM {
    // 路由按 partition_id 取模（对齐 C++），故用不同分区号分散到各 shard。
    let table = TableIdent::new(format!("stress{index}"), index as u32 + 1).unwrap();
    let shard = usize::from(table.shard_index(SHARD_NUM));
    if count_per_shard[shard] < TABLE_NUM / usize::from(SHARD_NUM) {
      count_per_shard[shard] += 1;
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

fn writer_key(worker: usize, index: usize) -> Vec<u8> {
  format!("w:{worker}:{index:04}").into_bytes()
}

fn writer_value(worker: usize, index: usize) -> Vec<u8> {
  format!("value:{worker}:{index}").into_bytes()
}

#[test]
fn concurrent_put_get_scan_consistency_and_metrics() {
  let root = TempRoot::new("stress");
  pollster::block_on(async {
    let runtime = Arc::new(
      Runtime::start(RuntimeConfig {
        num_threads: SHARD_NUM,
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
        queue_size: 256,
      })
      .unwrap(),
    );
    for shard_id in 0..u32::from(SHARD_NUM) {
      runtime.attach_shard(shard_id, "main", 1).await.unwrap();
    }
    let table_li = tables_per_shard();
    for (table_index, table) in table_li.iter().enumerate() {
      let batch: Vec<BatchEntry> = (0..BASE_NUM)
        .map(|index| BatchEntry::upsert(base_key(index), base_value(table_index, index), 1))
        .collect();
      runtime.put(table, batch).await.unwrap();
    }
    let before = runtime.metrics();

    let put_num = Arc::new(AtomicU64::new(0));
    let get_num = Arc::new(AtomicU64::new(0));
    let scan_num = Arc::new(AtomicU64::new(0));

    let mut writer_li = Vec::new();
    for worker in 0..4usize {
      let runtime = runtime.clone();
      let table_li = table_li.clone();
      let put_num = put_num.clone();
      let get_num = get_num.clone();
      writer_li.push(thread::spawn(move || {
        pollster::block_on(async {
          for table_offset in [worker, worker + 4] {
            let table = &table_li[table_offset];
            for batch_index in 0..BATCH_NUM {
              let batch: Vec<BatchEntry> = (0..BATCH_SIZE)
                .map(|index| {
                  let key_index = batch_index * BATCH_SIZE + index;
                  BatchEntry::upsert(
                    writer_key(worker, key_index),
                    writer_value(worker, key_index),
                    2,
                  )
                })
                .collect();
              runtime.put(table, batch).await.unwrap();
              put_num.fetch_add(1, Ordering::Relaxed);
              let key_index = batch_index * BATCH_SIZE;
              assert_eq!(
                runtime
                  .get(table, &writer_key(worker, key_index))
                  .await
                  .unwrap(),
                Some(writer_value(worker, key_index))
              );
              get_num.fetch_add(1, Ordering::Relaxed);
            }
          }
        });
      }));
    }

    let mut reader_li = Vec::new();
    for _ in 0..READER_NUM {
      let runtime = runtime.clone();
      let table_li = table_li.clone();
      let get_num = get_num.clone();
      let scan_num = scan_num.clone();
      reader_li.push(thread::spawn(move || {
        pollster::block_on(async {
          for iteration in 0..READ_ITER {
            let table_index = iteration % table_li.len();
            let table = &table_li[table_index];
            for index in 0..BASE_NUM {
              assert_eq!(
                runtime.get(table, &base_key(index)).await.unwrap(),
                Some(base_value(table_index, index))
              );
              get_num.fetch_add(1, Ordering::Relaxed);
            }
            let entry_li = runtime.scan(table, b"b:", b"c:", 0).await.unwrap();
            assert_eq!(entry_li.len(), BASE_NUM);
            for (key, value) in entry_li {
              assert!(key.starts_with(b"b:"));
              assert!(value.starts_with(b"base:"));
            }
            scan_num.fetch_add(1, Ordering::Relaxed);
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
    let after = runtime.metrics();

    let delta = |op: Op| after.total_requests(op) - before.total_requests(op);
    assert_eq!(delta(Op::Put), put_num.load(Ordering::Relaxed));
    assert_eq!(delta(Op::Get), get_num.load(Ordering::Relaxed));
    assert_eq!(delta(Op::Scan), scan_num.load(Ordering::Relaxed));
    assert_eq!(delta(Op::Put), 80);
    assert_eq!(
      delta(Op::Get),
      80 + READER_NUM as u64 * READ_ITER as u64 * BASE_NUM as u64
    );
    assert_eq!(delta(Op::Scan), READER_NUM as u64 * READ_ITER as u64);
    assert_eq!(after.total_errors(Op::Put), before.total_errors(Op::Put));
    assert_eq!(after.total_errors(Op::Get), before.total_errors(Op::Get));
    assert_eq!(after.total_errors(Op::Scan), before.total_errors(Op::Scan));

    for op in [Op::Put, Op::Get, Op::Scan] {
      let latency_count: u64 = after
        .shard_li
        .iter()
        .map(|shard| shard.op(op).latency.count)
        .sum();
      assert_eq!(latency_count, after.total_requests(op));
    }
    for (index, shard_before) in before.shard_li.iter().enumerate() {
      let shard_after = &after.shard_li[index];
      for op in [Op::Put, Op::Get, Op::Scan] {
        assert!(
          shard_after.op(op).total > shard_before.op(op).total,
          "shard {} did not serve op {}",
          shard_after.shard_id,
          op.as_str()
        );
      }
      assert!(shard_after.page_writes > shard_before.page_writes);
    }

    // 全量校验：base + 每个 worker 写入的 key 都存在且值正确。
    for (table_index, table) in table_li.iter().enumerate() {
      let entry_li = runtime.scan(table, b"", b"", 0).await.unwrap();
      let entry_map: HashMap<Vec<u8>, Vec<u8>> = entry_li.into_iter().collect();
      assert_eq!(entry_map.len(), BASE_NUM + BATCH_NUM * BATCH_SIZE);
      for index in 0..BASE_NUM {
        assert_eq!(
          entry_map.get(&base_key(index)),
          Some(&base_value(table_index, index))
        );
      }
      let worker = table_index % 4;
      for key_index in 0..BATCH_NUM * BATCH_SIZE {
        assert_eq!(
          entry_map.get(&writer_key(worker, key_index)),
          Some(&writer_value(worker, key_index))
        );
      }
    }

    let runtime = Arc::try_unwrap(runtime).ok().expect("runtime still shared");
    runtime.shutdown().await.unwrap();
  });
}
