# テスト

## 地元

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` グローバル レジストリを共有するユースケース。 `cargo test` を直接使用する場合に追加します
`--test-threads=1`。

## Linuxコンテナ

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

このスクリプトは、zigbuild を使用して Linux テスト バイナリをクロスコンパイルし、それを最小限の Debian イメージに入れて実行します。

## CI

`ci.yml` fmt、clippy、フル テスト、および RustFS S3 統合テストを実行します。