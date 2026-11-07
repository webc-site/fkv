# proba

## local

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Rexistro global compartido de casos de uso; engadir ao usar `cargo test` directamente
`--test-threads=1`.

## Contenedores Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

O script usa zigbuild para compilar de xeito cruzado o binario de proba de Linux e, a continuación, colócao na imaxe mínima de Debian para executalo.

## CI

`ci.yml` Executa fmt, clippy, probas completas e probas de integración de RustFS S3.