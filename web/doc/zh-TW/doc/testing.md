# 測試

## 本地

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` 用例共享全域註冊表；直接用 `cargo test` 時加
`--test-threads=1`。

## Linux 容器

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

腳本用 zigbuild 交叉編譯 Linux 測試二進制，再放進最小 Debian 鏡像運行。

## CI

`ci.yml` 跑 fmt、clippy、全量測試與 RustFS S3 整合測試。