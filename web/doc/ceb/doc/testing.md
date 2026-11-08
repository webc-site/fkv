# pagsulay

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Gamita ang case shared global registry; idugang kon gamiton ang `cargo test` direkta
`--test-threads=1`.

## Mga sudlanan sa Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Gigamit sa script ang zigbuild aron ma-cross-compile ang Linux test binary, ug dayon ibutang kini sa gamay nga imahe sa Debian aron madagan.

## CI

`ci.yml` Run fmt, clippy, full tests ug RustFS S3 integration tests.