# test

## lokalni

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Dijeljeni globalni registar slučaja korištenja; dodati kada izravno koristite `cargo test`
`--test-threads=1`.

## Linux spremnici

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skripta koristi zigbuild za unakrsno kompajliranje binarne datoteke za testiranje Linuxa, a zatim je stavlja u minimalnu Debian sliku za pokretanje.

## CI

`ci.yml` Pokrenite fmt, clippy, pune testove i testove integracije RustFS S3.
`linux-test.yml` Vraćeno svake noći u Ubuntu 24.04 LTS i potvrđeno 4 KiB stranice.