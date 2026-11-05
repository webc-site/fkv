# тэст

## мясцовыя

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Агульны глабальны рэестр варыянтаў выкарыстання; дадаць пры непасрэдным выкарыстанні `cargo test`
`--test-threads=1`.

## Linux кантэйнеры

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Скрыпт выкарыстоўвае zigbuild для крос-кампіляцыі тэставага бінарнага файла Linux, а затым змяшчае яго ў мінімальны вобраз Debian для запуску.

## CI

`ci.yml` Запусціце fmt, clippy, поўныя тэсты і інтэграцыйныя тэсты RustFS S3.
`linux-test.yml` Штовечар вяртаўся ў Ubuntu 24.04 LTS і заяўляў старонкі памерам 4 КіБ.