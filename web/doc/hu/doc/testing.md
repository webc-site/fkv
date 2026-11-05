# teszt

## helyi

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` A megosztott globális nyilvántartás használata; adjon hozzá a `cargo test` közvetlen használatakor
`--test-threads=1`.

## Linux konténerek

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

A szkript a zigbuild használatával keresztbe fordítja a Linux teszt binárist, majd a minimális Debian lemezképbe helyezi a futtatáshoz.

## CI

`ci.yml` Fmt, clippy, teljes tesztek és RustFS S3 integrációs tesztek futtatása.
`linux-test.yml` Esténként visszaküldték az Ubuntu 24.04 LTS-ben, és 4 KiB-os oldalt adtak vissza.