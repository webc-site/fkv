# scripts/ci

CI 与本地共用的原生依赖 / RustFS 脚本，不依赖 Docker。

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
