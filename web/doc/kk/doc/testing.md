# сынақ

## жергілікті

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Ортақ жаһандық тізілімді пайдалану жағдайы; тікелей `cargo test` пайдаланған кезде қосыңыз
`--test-threads=1`.

## Linux контейнерлері

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Сценарий Linux сынақ екілік файлын кросс-компиляциялау үшін zigbuild пайдаланады, содан кейін оны іске қосу үшін минималды Debian кескініне қояды.

## CI

`ci.yml` fmt, clippy, толық сынақтарды және RustFS S3 интеграция сынақтарын іске қосыңыз.