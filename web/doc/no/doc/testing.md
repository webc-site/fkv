# test

## lokale

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Delt globalt register for brukssaker; legg til når du bruker `cargo test` direkte
`--test-threads=1`.

## Linux-beholdere

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skriptet bruker zigbuild for å krysskompilere Linux-testbinæren, og legger den deretter inn i det minimale Debian-bildet som skal kjøres.

## CI

`ci.yml` Kjør fmt, clippy, fullstendige tester og RustFS S3-integrasjonstester.