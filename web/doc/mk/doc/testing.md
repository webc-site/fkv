# тест

## локални

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Употреба на споделен глобален регистар; додадете кога директно го користите `cargo test`
`--test-threads=1`.

## Линукс контејнери

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Скриптата користи zigbuild за вкрстено компајлирање на тест-бинарната верзија на Linux, а потоа ја става во минималната слика на Debian за да се изврши.

## CI

`ci.yml` Стартувај fmt, clippy, целосни тестови и RustFS S3 тестови за интеграција.