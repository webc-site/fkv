# test

## yerli

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Paylaşılan qlobal reyestrdən istifadə halı; birbaşa `cargo test` istifadə edərkən əlavə edin
`--test-threads=1`.

## Linux konteynerləri

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skript Linux test binarını çarpaz tərtib etmək üçün zigbuild-dən istifadə edir və sonra onu işlətmək üçün minimal Debian şəklinə qoyur.

## CI

`ci.yml` fmt, clippy, tam testlər və RustFS S3 inteqrasiya testlərini işə salın.
`linux-test.yml` Hər gecə Ubuntu 24.04 LTS-də geri qayıdıb və 4 KiB səhifəsini təsdiqləyib.