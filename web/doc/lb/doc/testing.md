# testen

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Benotzt Fall gedeelt global Registry; add wann Dir `cargo test` direkt benotzt
`--test-threads=1`.

## Linux Container

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

De Skript benotzt Zigbuild fir de Linux Test Binär ze kompiléieren, a setzt se dann an dat minimalt Debian Bild fir ze lafen.

## CI

`ci.yml` Run fmt, clippy, voll Tester a RustFS S3 Integratiounstester.