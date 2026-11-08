# تست کنید

## محلی

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` از رجیستری جهانی مشترک استفاده کنید. هنگام استفاده مستقیم از `cargo test` اضافه کنید
`--test-threads=1`.

## کانتینرهای لینوکس

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

این اسکریپت از zigbuild برای کامپایل متقابل تست باینری لینوکس استفاده می‌کند و سپس آن را در تصویر حداقل دبیان قرار می‌دهد تا اجرا شود.

## CI

`ci.yml` تست های fmt، clippy، full test و RustFS S3 را اجرا کنید.