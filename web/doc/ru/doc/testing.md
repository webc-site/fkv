# тест

## местный

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Вариант использования общего глобального реестра; добавить при непосредственном использовании `cargo test`
`--test-threads=1`.

## Контейнеры Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Сценарий использует zigbuild для кросс-компиляции тестового двоичного файла Linux, а затем помещает его в минимальный образ Debian для запуска.

## CI

`ci.yml` Запустите fmt, clippy, полные тесты и тесты интеграции RustFS S3.