# testa

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Use case delat globalt register; lägg till när du använder `cargo test` direkt
`--test-threads=1`.

## Linux-behållare

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skriptet använder zigbuild för att korskompilera Linux-testbinären och placerar den sedan i den minimala Debian-avbildningen som ska köras.

## CI

`ci.yml` Kör fmt, clippy, fullständiga tester och RustFS S3-integreringstester.
`linux-test.yml` Återkom varje natt i Ubuntu 24.04 LTS och hävdade 4 KiB-sidor.