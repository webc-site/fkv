# fkv

纯 Rust 的嵌入式 KV 存储引擎。

- `crate/`：按 foundation / io / core / format / engine / facade / perf 分层，统一 `fkv_` 前缀。
- `doc/architecture/`：分层、执行模型、IO、格式、持久化与缺口。
- `tool/`：仓库守卫脚本。
- `docker/`：Linux 测试与 S3 集成测试环境。

## 构建与测试

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`./test.sh` 使用 cargo nextest，每个测试独立进程。直接 `cargo test` 跑 `fkv_failpoint`
相关用例需加 `--test-threads=1`（全局注入注册表会互相干扰）。

Linux 交叉编译后进容器运行（不需要在容器里编译）：

```bash
cargo install cargo-zigbuild
./docker-test.sh                 # 全部 crate
./docker-test.sh -p fkv_store    # 指定 crate
```

zig / python3 / docker / cargo-zigbuild 缺失时脚本会自动用 mise / brew / cargo 安装；
docker 缺失时通过 `brew install --cask orbstack` + `orb start` 启动。

## CI

- `ci.yml`：fmt / clippy / 全量测试 / RustFS S3 集成测试，全部在 GitHub 托管
  runner 上原生执行；测试二进制不再打包成 artifact 或转交 Blacksmith。
- `linux-test.yml`：夜间与手动的 Ubuntu 24.04 LTS 回归，断言 4 KiB 页。
- `nvme.yml`：只构建并上传 `fkv_nvme_tests`（约 7 MB `.tar.zst`），由
  Blacksmith/自托管本地 NVMe runner 下载执行；普通 `ci.yml` 会排除该 crate。
- `release.yml`：仅在推送 `v*` tag（或手动 dispatch）时构建 `fkv_bench`
  x86_64 / aarch64，上传滚动的 `perf-latest` release。
- `web.yml`：构建并发布多语言文档站点。

CI 全程原生执行，不依赖 Docker。依赖安装与 RustFS 生命周期由
`scripts/ci/install.sh`、`scripts/ci/s3.sh` 统一提供，用法见
[`scripts/ci/README.md`](scripts/ci/README.md)。`docker/docker-compose.yml`
仅用于本地开发与调试。
