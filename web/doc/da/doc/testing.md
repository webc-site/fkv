# prøve

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Use case delt globalt register; tilføje, når du bruger `cargo test` direkte
`--test-threads=1`.

## Linux containere

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Scriptet bruger zigbuild til at krydskompilere Linux-testbinæren og sætter den derefter ind i det minimale Debian-image, der skal køres.

## CI

`ci.yml` Kør fmt, clippy, fulde tests og RustFS S3 integrationstest.