# testata

## paikallinen

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Käytä tapausten jaettua maailmanlaajuista rekisteriä; lisää, kun käytät `cargo test` suoraan
`--test-threads=1`.

## Linux-säiliöt

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Komentosarja käyttää zigbuildia Linuxin testibinaarin ristiin kääntämiseen ja laittaa sen sitten ajettavaksi minimaaliseen Debian-näköistiedostoon.

## CI

`ci.yml` Suorita fmt, clippy, täydet testit ja RustFS S3 -integraatiotestit.