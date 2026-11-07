# pārbaudi

## vietējā

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Izmantojiet gadījumu koplietoto globālo reģistru; pievienot, izmantojot `cargo test` tieši
`--test-threads=1`.

## Linux konteineri

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skripts izmanto zigbuild, lai savstarpēji kompilētu Linux testa bināro failu, un pēc tam ievieto to minimālajā Debian attēlā, lai to palaistu.

## CI

`ci.yml` Palaidiet fmt, clippy, pilnos testus un RustFS S3 integrācijas testus.