# тест

## місцевий

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Спільний глобальний реєстр варіантів використання; додати під час безпосереднього використання `cargo test`
`--test-threads=1`.

## Контейнери Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Сценарій використовує zigbuild для крос-компіляції тестового двійкового файлу Linux, а потім поміщає його в мінімальний образ Debian для запуску.

## CI

`ci.yml` Запустіть fmt, clippy, повні тести та тести інтеграції RustFS S3.
`linux-test.yml` Повертався щоночі в Ubuntu 24.04 LTS і заявляв 4 КБ сторінок.