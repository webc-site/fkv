//! `ManifestIo`：manifest / 旁路文件写入与列举，写成功即 durable。

use fkv_error::Result;
use fkv_failpoint::points::{APPEND_SYNC, APPEND_WRITE, ATOMIC_RENAME};
use fkv_io::ManifestIo;
use fkv_types::TableIdent;

use crate::{
  SimStore,
  op::Op,
  state::{log_op, log_op_payload},
};

impl ManifestIo for SimStore {
  async fn reset_manifest(&self, tbl: &TableIdent, size: u64) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ResetManifest {
        tbl: tbl.clone(),
        size,
      },
    )?;
    if let Some(bytes) = state.durable.manifest_map.get_mut(tbl) {
      bytes.truncate(size as usize);
    }
    Ok(())
  }

  async fn reset_manifest_file(&self, tbl: &TableIdent, name: &str, size: u64) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ResetManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
        size,
      },
    )?;
    let key = (tbl.clone(), name.to_owned());
    if let Some(bytes) = state.durable.manifest_file_map.get_mut(&key) {
      bytes.truncate(size as usize);
    }
    Ok(())
  }

  async fn append_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    fkv_failpoint::fail_point!(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::AppendManifest {
        tbl: tbl.clone(),
        len: record.len(),
      },
      Some(record),
    )?;
    let mut bytes = state.durable.manifest_map.remove(tbl).unwrap_or_default();
    bytes.extend_from_slice(record);
    state.durable.manifest_map.insert(tbl.clone(), bytes);
    Ok(())
  }

  async fn append_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    fkv_failpoint::fail_point!(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_WRITE);
    fkv_failpoint::kill_point(APPEND_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::AppendManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
        len: record.len(),
      },
      Some(record),
    )?;
    let key = (tbl.clone(), name.to_owned());
    let mut bytes = state
      .durable
      .manifest_file_map
      .remove(&key)
      .unwrap_or_default();
    bytes.extend_from_slice(record);
    state.durable.manifest_file_map.insert(key, bytes);
    Ok(())
  }

  async fn switch_manifest(&self, tbl: &TableIdent, record: &[u8]) -> Result<()> {
    fkv_failpoint::kill_point(ATOMIC_RENAME);
    fkv_failpoint::fail_point!(ATOMIC_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::SwitchManifest {
        tbl: tbl.clone(),
        len: record.len(),
      },
      Some(record),
    )?;
    // 对齐 `switch_manifest`（tmp → fdatasync → rename）：写成功即 durable。
    state
      .durable
      .manifest_map
      .insert(tbl.clone(), record.to_vec());
    Ok(())
  }

  async fn read_manifest(&self, tbl: &TableIdent) -> Result<Option<Vec<u8>>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadManifest { tbl: tbl.clone() },
    )?;
    Ok(state.durable.manifest_map.get(tbl).cloned())
  }

  async fn drop_manifest(&self, tbl: &TableIdent) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::DropManifest { tbl: tbl.clone() },
    )?;
    state.durable.manifest_map.remove(tbl);
    Ok(())
  }

  async fn write_manifest_file(&self, tbl: &TableIdent, name: &str, record: &[u8]) -> Result<()> {
    fkv_failpoint::kill_point(ATOMIC_RENAME);
    fkv_failpoint::fail_point!(ATOMIC_SYNC);
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WriteManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
        len: record.len(),
      },
      Some(record),
    )?;
    state
      .durable
      .manifest_file_map
      .insert((tbl.clone(), name.to_owned()), record.to_vec());
    Ok(())
  }

  async fn list_manifest_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListManifestFileNameLi { tbl: tbl.clone() },
    )?;
    let mut names: Vec<String> = Vec::new();
    for key in state.durable.manifest_file_map.keys() {
      // 只返回真 manifest：`segcomplete*` 等旁路名不得混入 GC 的 manifest 名单。
      if &key.0 == tbl && fkv_types::parse_manifest_file_name(&key.1).is_some() {
        names.push(key.1.clone());
      }
    }
    names.sort();
    names.dedup();
    Ok(names)
  }

  /// 非 manifest 命名规则的旁路文件（如 segment 完成标记）。
  async fn list_sidecar_file_name_li(&self, tbl: &TableIdent) -> Result<Vec<String>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ListSidecarFileNameLi { tbl: tbl.clone() },
    )?;
    let mut names: Vec<String> = state
      .durable
      .manifest_file_map
      .keys()
      .filter(|key| &key.0 == tbl && fkv_types::parse_manifest_file_name(&key.1).is_none())
      .map(|key| key.1.clone())
      .collect();
    names.sort_unstable();
    names.dedup();
    Ok(names)
  }

  async fn remove_sidecar_file_name(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveSidecarFileName {
        tbl: tbl.clone(),
        name: name.to_owned(),
      },
    )?;
    state
      .durable
      .manifest_file_map
      .remove(&(tbl.clone(), name.to_owned()));
    Ok(())
  }

  async fn read_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<Option<Vec<u8>>> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::ReadManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
      },
    )?;
    let key = (tbl.clone(), name.to_owned());
    Ok(state.durable.manifest_file_map.get(&key).cloned())
  }

  async fn remove_manifest_file(&self, tbl: &TableIdent, name: &str) -> Result<()> {
    let mut state = self.lock();
    log_op(
      &self.config,
      &mut state,
      Op::RemoveManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
      },
    )?;
    state
      .durable
      .manifest_file_map
      .remove(&(tbl.clone(), name.to_owned()));
    Ok(())
  }

  /// Sim 无远端：晋升种子与具名写同一 op（崩溃丢种子可由回退链幂等重建）。
  async fn seed_local_manifest_file(
    &self,
    tbl: &TableIdent,
    name: &str,
    bytes: &[u8],
  ) -> Result<()> {
    let mut state = self.lock();
    log_op_payload(
      &self.config,
      &mut state,
      Op::WriteManifestFile {
        tbl: tbl.clone(),
        name: name.to_owned(),
        len: bytes.len(),
      },
      Some(bytes),
    )?;
    state
      .durable
      .manifest_file_map
      .insert((tbl.clone(), name.to_owned()), bytes.to_vec());
    Ok(())
  }
}
