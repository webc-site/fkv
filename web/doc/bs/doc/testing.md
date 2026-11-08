# test

## lokalni

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Koristi zajednički globalni registar; dodajte kada direktno koristite `cargo test`
`--test-threads=1`.

## Linux kontejneri

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skripta koristi zigbuild za unakrsno kompajliranje Linux test binarnog programa, a zatim ga stavlja u minimalnu Debian sliku za pokretanje.

## CI

`ci.yml` Pokrenite fmt, clippy, pune testove i RustFS S3 integracijske testove.