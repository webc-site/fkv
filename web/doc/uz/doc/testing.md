# sinov

## mahalliy

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Umumiy global registrdan foydalanish; to'g'ridan-to'g'ri `cargo test` dan foydalanganda qo'shing
`--test-threads=1`.

## Linux konteynerlari

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skript Linux test binarini o'zaro kompilyatsiya qilish uchun zigbuild-dan foydalanadi va keyin uni ishga tushirish uchun minimal Debian tasviriga qo'yadi.

## CI

`ci.yml` fmt, clippy, to'liq testlar va RustFS S3 integratsiya testlarini ishga tushiring.