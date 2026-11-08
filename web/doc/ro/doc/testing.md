# test

## local

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Registrul global partajat de cazuri de utilizare; adăugați când utilizați `cargo test` direct
`--test-threads=1`.

## Containere Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Scriptul folosește zigbuild pentru a compila binarul de testare Linux și apoi îl pune în imaginea Debian minimă pentru a rula.

## CI

`ci.yml` Rulați teste fmt, clippy, complete și teste de integrare RustFS S3.