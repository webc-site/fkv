# Test

## Local

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Use case shared global registry; add when using `cargo test` directly
`--test-threads=1`.

## Linux containers

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

The script uses zigbuild to cross-compile the Linux test binary, and then puts it into the minimal Debian image to run.

## CI

`ci.yml` Run fmt, clippy, full tests and RustFS S3 integration tests.