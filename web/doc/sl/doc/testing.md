# test

## lokalni

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Skupni globalni register primerov uporabe; dodaj pri neposredni uporabi `cargo test`
`--test-threads=1`.

## Vsebniki Linuxa

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skript uporablja zigbuild za navzkrižno prevajanje testne binarne datoteke Linux in jo nato postavi v minimalno sliko Debian za izvajanje.

## CI

`ci.yml` Zaženite fmt, clippy, polne teste in integracijske teste RustFS S3.