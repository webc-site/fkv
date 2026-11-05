# тест

## местен

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Споделен глобален регистър на случая на използване; добавете, когато използвате `cargo test` директно
`--test-threads=1`.

## Linux контейнери

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Скриптът използва zigbuild за кръстосано компилиране на тестовия двоичен файл на Linux и след това го поставя в минималното изображение на Debian за изпълнение.

## CI

`ci.yml` Изпълнете fmt, clippy, пълни тестове и RustFS S3 интеграционни тестове.
`linux-test.yml` Връща се всяка нощ в Ubuntu 24.04 LTS и заявява 4 KiB страници.