# proba

## tokikoa

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Erabilera-kasuaren erregistro global partekatua; gehitu `cargo test` zuzenean erabiltzean
`--test-threads=1`.

## Linux edukiontziak

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Scriptak zigbuild erabiltzen du Linux probaren bitarra gurutzatzeko konpilatzeko, eta gero Debian irudi minimoan jartzen du exekutatzeko.

## CI

`ci.yml` Exekutatu fmt, clippy, proba osoak eta RustFS S3 integrazio probak.
`linux-test.yml` Ubuntu 24.04 LTS-n gauero itzultzen da eta 4 KiB orrialde aldarrikatzen ditu.