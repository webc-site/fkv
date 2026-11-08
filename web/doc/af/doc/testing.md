# toets

## plaaslik

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Gebruik geval gedeelde globale register; voeg by wanneer jy `cargo test` direk gebruik
`--test-threads=1`.

## Linux houers

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Die skrip gebruik zigbuild om die Linux-toetsbinêre te kruis-samestel, en plaas dit dan in die minimale Debian-beeld om te hardloop.

## CI

`ci.yml` Begin fmt, clippy, volledige toetse en RustFS S3-integrasietoetse.