# tes

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Kasus penggunaan registri global bersama; tambahkan saat menggunakan `cargo test` secara langsung
`--test-threads=1`.

## wadah Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skrip ini menggunakan zigbuild untuk mengkompilasi silang biner pengujian Linux, dan kemudian memasukkannya ke dalam image Debian minimal untuk dijalankan.

## CI

`ci.yml` Jalankan pengujian fmt, clippy, pengujian lengkap, dan pengujian integrasi RustFS S3.