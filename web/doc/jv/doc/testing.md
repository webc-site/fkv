# tes

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Gunakake registri global sing dienggo bareng kasus; tambah nalika nggunakake `cargo test` langsung
`--test-threads=1`.

## kontaner Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skrip kasebut nggunakake zigbuild kanggo kompilasi silang binar tes Linux, banjur dilebokake ing gambar Debian minimal kanggo mbukak.

## CI

`ci.yml` Jalanake fmt, clippy, tes lengkap lan tes integrasi RustFS S3.