# test

## lokaal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Use case gedeeld mondiaal register; toevoegen als je `cargo test` rechtstreeks gebruikt
`--test-threads=1`.

## Linux-containers

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Het script gebruikt zigbuild om het binaire Linux-testbestand te cross-compileren en plaatst het vervolgens in de minimale Debian-image om uit te voeren.

## CI

`ci.yml` Voer fmt-, clippy-, volledige tests en RustFS S3-integratietests uit.
`linux-test.yml` Keerde elke nacht terug in Ubuntu 24.04 LTS en beweerde 4 KiB-pagina's.