use motedb::{Database, types::Value};

use crate::{
  error::Result,
  util::{now_secs, text_of},
};

/// path_prefix -> (crate, layer)。最长前缀优先。
const RULE_LI: &[(&str, &str, &str)] = &[
  ("include/types.h", "fkv_types", "foundation"),
  ("include/error.h", "fkv_error", "foundation"),
  ("include/coding.h", "fkv_codec", "foundation"),
  ("include/kv_options.h", "fkv_config", "foundation"),
  ("include/common.h", "fkv_types", "foundation"),
  ("include/manifest_buffer.h", "fkv_manifest", "format"),
  ("include/compression.h", "fkv_compress", "format"),
  ("src/compression.cpp", "fkv_compress", "format"),
  ("include/storage/page.h", "fkv_page", "format"),
  ("include/storage/data_page.h", "fkv_page", "format"),
  ("include/storage/data_page_builder.h", "fkv_page", "format"),
  ("include/storage/index_page_builder.h", "fkv_page", "format"),
  ("src/storage/page.cpp", "fkv_page", "format"),
  ("src/storage/data_page.cpp", "fkv_page", "format"),
  ("include/storage/page_mapper.h", "fkv_mapping", "core"),
  ("src/storage/page_mapper.cpp", "fkv_mapping", "core"),
  ("include/storage/mem_cached_page.h", "fkv_cache", "core"),
  ("include/storage/page_manager.h", "fkv_cache", "core"),
  ("src/storage/page_manager.cpp", "fkv_cache", "core"),
  ("include/storage/root_meta.h", "fkv_root", "core"),
  ("src/storage/root_meta.cpp", "fkv_root", "core"),
  ("include/write_tree_stack.h", "fkv_btree", "core"),
  ("include/async_io_manager.h", "fkv_io", "io"),
  ("src/async_io_manager.cpp", "fkv_io_compio", "io"),
  ("include/io_buffer_ref.h", "fkv_buf", "core"),
  ("include/io_string_buffer.h", "fkv_buf", "core"),
  ("include/direct_io_buffer.h", "fkv_buf", "core"),
  ("include/global_registered_memory.h", "fkv_large", "core"),
  ("src/global_registered_memory.cpp", "fkv_large", "core"),
  ("include/storage/object_store.h", "fkv_object", "io"),
  ("src/storage/object_store.cpp", "fkv_object", "io"),
  ("include/storage/cloud_backend.h", "fkv_object_s3", "io"),
  ("src/storage/cloud_backend.cpp", "fkv_object_s3", "io"),
  ("include/cloud_storage_service.h", "fkv_cloud", "engine"),
  ("src/cloud_storage_service.cpp", "fkv_cloud", "engine"),
  ("include/standby_service.h", "fkv_standby", "io"),
  ("src/standby_service.cpp", "fkv_standby", "io"),
  ("include/replayer.h", "fkv_recovery", "engine"),
  ("src/replayer.cpp", "fkv_recovery", "engine"),
  ("include/file_gc.h", "fkv_gc", "engine"),
  ("src/file_gc.cpp", "fkv_gc", "engine"),
  ("include/tasks/task.h", "fkv_task", "engine"),
  ("include/tasks/task_manager.h", "fkv_shard", "engine"),
  ("include/tasks/read_task.h", "fkv_task_read", "engine"),
  ("src/tasks/read_task.cpp", "fkv_task_read", "engine"),
  ("include/tasks/scan_task.h", "fkv_task_scan", "engine"),
  ("src/tasks/scan_task.cpp", "fkv_task_scan", "engine"),
  (
    "include/tasks/batch_write_task.h",
    "fkv_task_write",
    "engine",
  ),
  ("src/tasks/batch_write_task.cpp", "fkv_task_write", "engine"),
  ("include/tasks/write_task.h", "fkv_task_write", "engine"),
  ("src/tasks/write_task.cpp", "fkv_task_write", "engine"),
  ("include/tasks/reopen_task.h", "fkv_task_reopen", "engine"),
  ("src/tasks/reopen_task.cpp", "fkv_task_reopen", "engine"),
  (
    "include/tasks/background_write.h",
    "fkv_task_compact",
    "engine",
  ),
  (
    "src/tasks/background_write.cpp",
    "fkv_task_compact",
    "engine",
  ),
  (
    "include/tasks/archive_crond.h",
    "fkv_task_archive",
    "engine",
  ),
  ("src/tasks/archive_crond.cpp", "fkv_task_archive", "engine"),
  ("include/tasks/prewarm_task.h", "fkv_task_prewarm", "engine"),
  ("src/tasks/prewarm_task.cpp", "fkv_task_prewarm", "engine"),
  ("include/storage/shard.h", "fkv_shard", "engine"),
  ("src/storage/shard.cpp", "fkv_shard", "engine"),
  ("include/eloq_store.h", "fkv_store", "engine"),
  ("src/eloq_store.cpp", "fkv_store", "engine"),
];

pub fn seed(db: &Database) -> Result<usize> {
  let tx = db.begin_transaction()?;
  db.execute("DELETE FROM rule")?.materialize()?;
  for (prefix, crate_name, layer) in RULE_LI {
    db.execute_prepared(
      "INSERT INTO rule (id, prefix, crate, layer) VALUES (?, ?, ?, ?)",
      vec![
        Value::text((*prefix).to_owned()),
        Value::text((*prefix).to_owned()),
        Value::text((*crate_name).to_owned()),
        Value::text((*layer).to_owned()),
      ],
    )?
    .materialize()?;
  }
  db.commit_transaction(tx)?;
  Ok(RULE_LI.len())
}

pub fn apply(db: &Database, force: bool) -> Result<usize> {
  let tx = db.begin_transaction()?;
  let result = db
    .execute("SELECT id, path, crate FROM unit")?
    .materialize()?;
  let Some((_, row_li)) = result.select_rows() else {
    db.commit_transaction(tx)?;
    return Ok(0);
  };
  let mut update_count = 0;
  for row in row_li {
    let Some(id) = row.first().and_then(text_of) else {
      continue;
    };
    let Some(path) = row.get(1).and_then(text_of) else {
      continue;
    };
    let crate_name = row.get(2).and_then(text_of);
    if !force && crate_name.is_some_and(|value| !value.is_empty()) {
      continue;
    }
    let best = RULE_LI
      .iter()
      .filter(|(prefix, ..)| path.starts_with(prefix))
      .max_by_key(|(prefix, ..)| prefix.len());
    let Some((_, target, layer)) = best else {
      continue;
    };
    db.execute_prepared(
      "UPDATE unit SET crate = ?, layer = ?, updated_at = ? WHERE id = ?",
      vec![
        Value::text((*target).to_owned()),
        Value::text((*layer).to_owned()),
        Value::Integer(now_secs()),
        Value::text(id.to_owned()),
      ],
    )?
    .materialize()?;
    update_count += 1;
  }
  db.commit_transaction(tx)?;
  Ok(update_count)
}
