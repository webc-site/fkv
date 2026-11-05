# test

## místní

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Případ použití sdílený globální registr; přidat při přímém použití `cargo test`
`--test-threads=1`.

## Linuxové kontejnery

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skript používá zigbuild ke křížové kompilaci testovacího binárního systému Linuxu a poté jej vloží do minimálního obrazu Debianu, aby se spustil.

## CI

`ci.yml` Spusťte fmt, clippy, úplné testy a integrační testy RustFS S3.
`linux-test.yml` Vracelo se každou noc v Ubuntu 24.04 LTS a prosazovalo 4 KiB stránky.