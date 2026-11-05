# scripts/ci

CI 与本地共用的原生依赖 / RustFS 脚本，不依赖 Docker。

## NVMe 隔离测试

`crate/test/fkv_nvme_test` 只放必须跑在本地 NVMe / SSD 上的少量测试，
`publish = false`，普通 `ci.yml` 用 `--exclude fkv_nvme_test` 跳过。
`.github/workflows/nvme.yml` 在 GitHub 托管 runner 上执行
`cargo nextest archive -p fkv_nvme_test`（输出 `.tar.zst`，约 7 MB），
再交给 Blacksmith 下载执行。

runner 要求与变量：

- 4 KiB 页、本地非 rotational 磁盘；测试目录默认 `$GITHUB_WORKSPACE/.nvme-data`。
- 自托管 / 其他标签：设置仓库变量 `FKV_NVME_RUNNER` 覆盖
  `blacksmith-4vcpu-ubuntu-2604`，设置 `FKV_NVME_ROOT` 指定挂载点。
- workflow 固定 `FKV_REQUIRE_NVME=1`：设备不符会失败，不会把 skip 当通过。

```bash
bash scripts/ci/install.sh          # Linux 构建依赖；非 Linux 自动 skip
bash scripts/ci/s3.sh start         # 启动 RustFS 并导出 EKV_S3_*
bash scripts/ci/s3.sh status
bash scripts/ci/s3.sh stop
```

GitHub Actions 里 `s3.sh start` 会把 `EKV_S3_*` 写入 `$GITHUB_ENV`：

```yaml
- run: bash scripts/ci/install.sh
- run: bash scripts/ci/s3.sh start
- run: cargo nextest run -p fkv_cloud -p fkv_object_s3 -p fkv_runtime --all-features
- if: always()
  run: bash scripts/ci/s3.sh stop
```

参数、默认值、重试与缓存布局见脚本自身说明。`docker/docker-compose.yml` 仅用于本地开发。
