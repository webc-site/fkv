# test

## lokali

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Każ ta' użu reġistru globali kondiviż; żid meta tuża `cargo test` direttament
`--test-threads=1`.

## Kontenituri tal-Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

L-iskrittura juża zigbuild biex jikkompila l-binarju tat-test tal-Linux, u mbagħad ipoġġih fl-immaġni Debian minima biex taħdem.

## CI

`ci.yml` Mexxi fmt, clippy, testijiet sħaħ u testijiet ta' integrazzjoni RustFS S3.