# provë

## lokale

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Përdorimi i regjistrit global të përbashkët të rastit; shtoni kur përdorni `cargo test` drejtpërdrejt
`--test-threads=1`.

## Kontejnerët Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skripti përdor zigbuild për të ndërthurur binarin e testimit të Linux, dhe më pas e vendos atë në imazhin minimal të Debian për t'u ekzekutuar.

## CI

`ci.yml` Ekzekutoni testet fmt, clippy, të plota dhe testet e integrimit të RustFS S3.