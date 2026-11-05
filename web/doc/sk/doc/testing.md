# testovať

## miestne

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Prípad použitia zdieľaný globálny register; pridať pri priamom použití `cargo test`
`--test-threads=1`.

## Linuxové kontajnery

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skript používa zigbuild na krížovú kompiláciu testovacieho binárneho systému Linuxu a potom ho vloží do minimálneho obrazu Debianu na spustenie.

## CI

`ci.yml` Spustite fmt, clippy, úplné testy a integračné testy RustFS S3.
`linux-test.yml` Vrátil sa každú noc v Ubuntu 24.04 LTS a presadil 4 kB stránky.