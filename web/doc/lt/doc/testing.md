# bandymas

## vietinis

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Naudokite bendrinamą visuotinį registrą; pridėti tiesiogiai naudojant `cargo test`
`--test-threads=1`.

## Linux konteineriai

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Scenarijus naudoja „zigbuild“, kad sukompiliuotų „Linux“ bandomąjį dvejetainį failą, o tada įdeda jį į minimalų „Debian“ atvaizdą, kad jis būtų paleistas.

## CI

`ci.yml` Vykdykite fmt, clippy, pilnus testus ir RustFS S3 integravimo testus.
`linux-test.yml` Grąžinama kas vakarą Ubuntu 24.04 LTS ir tvirtinami 4 KiB puslapiai.