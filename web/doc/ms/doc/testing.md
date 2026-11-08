# ujian

## tempatan

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Gunakan pendaftaran global kongsi kes; tambah apabila menggunakan `cargo test` secara langsung
`--test-threads=1`.

## Bekas Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skrip menggunakan zigbuild untuk menyusun silang binari ujian Linux, dan kemudian memasukkannya ke dalam imej Debian yang minimum untuk dijalankan.

## CI

`ci.yml` Jalankan ujian fmt, clippy, penuh dan ujian integrasi RustFS S3.