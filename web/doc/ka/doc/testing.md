# ტესტი

## ადგილობრივი

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Use case shared გლობალური რეესტრი; დაამატეთ `cargo test`-ის პირდაპირ გამოყენებისას
`--test-threads=1`.

## Linux კონტეინერები

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

სკრიპტი იყენებს zigbuild-ს Linux-ის ტესტის ორობითი ჯვარედინი კომპილაციისთვის და შემდეგ აყენებს მას მინიმალურ Debian სურათში გასაშვებად.

## CI

`ci.yml` გაუშვით fmt, clippy, სრული ტესტები და RustFS S3 ინტეგრაციის ტესტები.