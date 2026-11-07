# 测试

## 本地

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` 用例共享全局注册表；直接用 `cargo test` 时加
`--test-threads=1`。

## Linux 容器

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

脚本用 zigbuild 交叉编译 Linux 测试二进制，再放进最小 Debian 镜像运行。

## CI

`ci.yml` 跑 fmt、clippy、全量测试与 RustFS S3 集成测试。
