# prova

## locals

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Registre global compartit de casos d'ús; afegir quan utilitzeu `cargo test` directament
`--test-threads=1`.

## Contenidors Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

L'script utilitza zigbuild per compilar encreuament el binari de prova de Linux i després el posa a la imatge mínima de Debian per executar-lo.

## CI

`ci.yml` Executeu fmt, clippy, proves completes i proves d'integració de RustFS S3.