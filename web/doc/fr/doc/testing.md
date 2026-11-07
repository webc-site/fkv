# test

## locale

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Utiliser le registre mondial de partage de cas ; ajouter lors de l'utilisation directe de `cargo test`
`--test-threads=1`.

## Conteneurs Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Le script utilise zigbuild pour compiler de manière croisée le binaire de test Linux, puis le place dans l'image Debian minimale à exécuter.

## CI

`ci.yml` Exécutez des tests fmt, clippy, complets et des tests d'intégration RustFS S3.