# prüfen

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Globale Registrierung für Anwendungsfallfreigabe; hinzufügen, wenn `cargo test` direkt verwendet wird
`--test-threads=1`.

## Linux-Container

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Das Skript verwendet zigbuild, um die Linux-Testbinärdatei zu kompilieren und fügt sie dann zur Ausführung in das minimale Debian-Image ein.

## CI

`ci.yml` Führen Sie FMT-, Clippy-, vollständige Tests und RustFS S3-Integrationstests aus.
`linux-test.yml` Jeden Abend in Ubuntu 24.04 LTS zurückgekehrt und 4 KiB-Seiten bestätigt.