# teste

## local

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Registro global compartilhado de casos de uso; adicione ao usar `cargo test` diretamente
<bid="1">.

## Contêineres Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

O script usa zigbuild para compilar o binário de teste do Linux e, em seguida, coloca-o na imagem mínima do Debian para execução.

## CI

`ci.yml` Execute testes fmt, clippy, completos e testes de integração RustFS S3.
`linux-test.yml` Retornado todas as noites no Ubuntu 24.04 LTS e afirmado páginas de 4 KiB.