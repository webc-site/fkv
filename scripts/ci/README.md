# scripts/ci

CI 与本地共用的原生依赖 / RustFS 脚本，不依赖 Docker。

## CI 接线的判定门禁

`ci.yml` lint job 除 fmt / clippy / orphan 外，还执行三项与收敛判定轮同口径的
门禁：`check_line_count.sh`（行数）、`check_layers.sh`（分层，内部会再跑一次
行数门禁）与 `cargo shear`（依赖，经 `taiki-e/install-action` 安装）。

死代码门禁 `bun sh/unused.js` **未接入 CI，保持轮间判定口径**：当前基线存在
2 条 trait impl 误报（`fkv_shard` 的 `partial_cmp`、`fkv_io_compio` 的
`as_uninit`），脚本按「有发现退出码 2」语义恒红，无法作为通过条件；待误报
消除（工具修复或源头标注）后再随 `oven-sh/setup-bun` 接线。

## NVMe 隔离测试

`crate/test/fkv_nvme_test` 只放必须跑在本地 NVMe / SSD 上的少量测试，
`publish = false`，普通 `ci.yml` 用 `--exclude fkv_nvme_test` 跳过。
`.github/workflows/nvme.yml` 在 GitHub 托管 runner 上执行
`cargo nextest archive -p fkv_nvme_test`（输出 `.tar.zst`，约 18 MB），
再交给 Blacksmith 下载执行。

runner 要求与变量：

- 4 KiB 页、本地磁盘；Blacksmith 等 VM 的 `vd*`/`xvd*` 即使 `rotational=1`
  也按本地 SSD 接受（仅功能测试；性能门槛仍要求 `rotational=0`）。
  测试目录默认 `$GITHUB_WORKSPACE/.nvme-data`。
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
