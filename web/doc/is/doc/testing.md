# próf

## staðbundið

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Notaðu sameiginlega alþjóðlega skráningu; bæta við þegar þú notar `cargo test` beint
`--test-threads=1`.

## Linux gámar

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Handritið notar zigbuild til að krosssamsetja Linux próf tvöfaldan, og setur það síðan í lágmarks Debian mynd til að keyra.

## CI

`ci.yml` Keyrðu fmt, clippy, full próf og RustFS S3 samþættingarpróf.
`linux-test.yml` Skilaði á hverju kvöldi í Ubuntu 24.04 LTS og fullyrtu 4 KiB síður.