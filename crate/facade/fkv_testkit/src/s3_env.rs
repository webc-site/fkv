//! S3 测试环境协议（`EKV_S3_*` env）的单一读取源。
//!
//! 写入侧（改 env 名或任一回退默认值时须同步这两处）：
//! - `scripts/ci/s3.sh`：`_s3_write_env`（cache 文件 / `GITHUB_ENV` / export 三形态）；
//! - `docker/docker-compose.yml`：`test.environment`（不写 region，走 `us-east-1` 回退）。
//!
//! 读取侧统一走 [`s3_env`]；`EKV_S3_ENDPOINT` 缺失时返回 `None`，集成测试
//! 约定据此跳过用例而非失败。

use std::env::var;

/// env 名（与写入侧一致，缺一即漏改）。
const ENV_ENDPOINT: &str = "EKV_S3_ENDPOINT";
const ENV_BUCKET: &str = "EKV_S3_BUCKET";
const ENV_REGION: &str = "EKV_S3_REGION";
const ENV_ACCESS_KEY: &str = "EKV_S3_ACCESS_KEY";
const ENV_SECRET_KEY: &str = "EKV_S3_SECRET_KEY";

/// 回退默认值（RustFS / minio 本地端点的约定值）。
const DEFAULT_BUCKET: &str = "fkv-test";
const DEFAULT_REGION: &str = "us-east-1";
const DEFAULT_ACCESS_KEY: &str = "minioadmin";
const DEFAULT_SECRET_KEY: &str = "minioadmin";

/// `EKV_S3_*` env 解析出的 S3 测试环境。
///
/// 有意不依赖 `fkv_object_s3::S3Config`（testkit 不引 io 层）；各消费方
/// 集成测试自行做几行字段映射。
#[derive(Debug, Clone)]
pub struct S3Env {
  pub endpoint: String,
  pub bucket: String,
  pub region: String,
  pub access_key: String,
  pub secret_key: String,
  /// 测试端点是 http://（rustfs / minio），不校验 TLS。
  pub allow_http: bool,
}

/// 读取 S3 测试环境：`EKV_S3_ENDPOINT` 缺失时返回 `None`（集成测试跳过），
/// 其余 env 缺失时用 RustFS / minio 约定的回退默认值。
pub fn s3_env() -> Option<S3Env> {
  Some(S3Env {
    endpoint: var(ENV_ENDPOINT).ok()?,
    bucket: var(ENV_BUCKET).unwrap_or_else(|_| DEFAULT_BUCKET.to_owned()),
    region: var(ENV_REGION).unwrap_or_else(|_| DEFAULT_REGION.to_owned()),
    access_key: var(ENV_ACCESS_KEY).unwrap_or_else(|_| DEFAULT_ACCESS_KEY.to_owned()),
    secret_key: var(ENV_SECRET_KEY).unwrap_or_else(|_| DEFAULT_SECRET_KEY.to_owned()),
    allow_http: true,
  })
}
