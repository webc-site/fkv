//! 隔离的 NVMe / 本地 SSD 测试。
//!
//! 普通 CI 在 GitHub 托管 runner 上跑；这个 crate 只由 `nvme.yml` 归档，
//! 在带本地 NVMe 的 Blacksmith runner 上执行。`FKV_REQUIRE_NVME=1` 时设备
//! 不满足条件会直接失败，避免把“跳过”误当成通过。

use std::{
  env,
  fs::{create_dir_all, remove_dir_all},
  path::{Path, PathBuf},
  process::Command,
};

use fkv_btree::BatchEntry;
use fkv_io_compio::CompioStore;
use fkv_store::Store;
use fkv_types::{DEFAULT_PAGES_PER_FILE_SHIFT, TableIdent};

const PAGE_SIZE: usize = 4096;
const RESTART_INTERVAL: u16 = 16;

fn require_nvme() -> bool {
  matches!(env::var("FKV_REQUIRE_NVME").as_deref(), Ok("1"))
}

fn root(tag: &str) -> PathBuf {
  let base = env::var_os("FKV_NVME_ROOT")
    .map(PathBuf::from)
    .unwrap_or_else(|| env::temp_dir().join("fkv-nvme-tests"));
  base.join(format!("{tag}-{}", std::process::id()))
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
  let output = Command::new(program).args(args).output().ok()?;
  output
    .status
    .success()
    .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn block_device(source: &str) -> String {
  let path = Path::new(source);
  let base = path
    .file_name()
    .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
  // 分区没有自己的 queue 属性，优先取整盘名。
  if !Path::new(&format!("/sys/block/{base}/queue/rotational")).exists()
    && let Some(parent) = command_stdout("lsblk", &["-no", "PKNAME", source])
    && !parent.is_empty()
  {
    return parent.lines().next().unwrap_or_default().to_owned();
  }
  base
}

/// 返回设备检查结论；`FKV_REQUIRE_NVME=1` 时不满足条件直接 panic。
fn ensure_nvme(root: &Path) -> bool {
  let mut reason = None;
  if !cfg!(target_os = "linux") {
    reason = Some("非 Linux".to_owned());
  } else if let Some(page) = command_stdout("getconf", &["PAGESIZE"]) {
    if page != PAGE_SIZE.to_string() {
      reason = Some(format!("PAGESIZE={page}，需要 {PAGE_SIZE}"));
    }
  } else {
    reason = Some("无法读取 PAGESIZE".to_owned());
  }

  if reason.is_none() {
    create_dir_all(root).expect("创建 NVMe 测试目录");
    let source = command_stdout(
      "findmnt",
      &["-T", &root.display().to_string(), "-no", "SOURCE"],
    )
    .filter(|text| !text.is_empty())
    .or_else(|| {
      command_stdout("df", &["--output=source", &root.display().to_string()])
        .and_then(|text| text.lines().last().map(str::to_owned))
    });
    match source {
      Some(source) if !source.is_empty() => {
        let dev = block_device(&source);
        let rotational = command_stdout("cat", &[&format!("/sys/block/{dev}/queue/rotational")]);
        match rotational.as_deref() {
          Some("0") => {}
          Some(other) => reason = Some(format!("{dev} rotational={other}")),
          None => reason = Some(format!("无法读取 {dev} 的 rotational")),
        }
      }
      _ => reason = Some("无法确认测试目录所在块设备".to_owned()),
    }
  }

  match reason {
    None => true,
    Some(reason) if require_nvme() => panic!("NVMe 测试环境不满足：{reason}"),
    Some(reason) => {
      eprintln!("跳过 NVMe 测试：{reason}（设置 FKV_REQUIRE_NVME=1 可强制失败）");
      false
    }
  }
}

#[test]
fn device_is_4k_local_ssd() {
  let root = root("device");
  let _ = ensure_nvme(&root);
  let _ = remove_dir_all(&root);
}

#[test]
fn engine_smoke_on_nvme() {
  let root = root("roundtrip");
  if !ensure_nvme(&root) {
    return;
  }
  pollster::block_on(async {
    let tbl = TableIdent::new("nvme", 1).unwrap();
    let io = CompioStore::new(root.clone(), PAGE_SIZE, DEFAULT_PAGES_PER_FILE_SHIFT);
    let mut store = Store::new(io, PAGE_SIZE, RESTART_INTERVAL);
    let batch = (0..256)
      .map(|index| {
        BatchEntry::upsert(
          format!("k{index:04}").into_bytes(),
          vec![b'v'; PAGE_SIZE],
          1,
        )
      })
      .collect::<Vec<_>>();
    store.put(&tbl, &batch).await.unwrap();
    assert_eq!(
      store.get(&tbl, b"k0128").await.unwrap(),
      Some(vec![b'v'; PAGE_SIZE])
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 256);

    store.reopen(&tbl).await.unwrap();
    assert_eq!(
      store.get(&tbl, b"k0000").await.unwrap(),
      Some(vec![b'v'; PAGE_SIZE])
    );
    assert_eq!(store.scan(&tbl, b"", b"", 0).await.unwrap().len(), 256);
  });
  remove_dir_all(&root).unwrap();
}
