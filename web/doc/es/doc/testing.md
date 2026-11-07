# prueba

## local

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Caso de uso de registro global compartido; agregar cuando se usa `cargo test` directamente
`--test-threads=1`.

## Contenedores de Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

El script utiliza zigbuild para realizar una compilación cruzada del binario de prueba de Linux y luego lo coloca en la imagen mínima de Debian para ejecutar.

## CI

`ci.yml` Ejecute pruebas fmt, clippy, pruebas completas y pruebas de integración RustFS S3.