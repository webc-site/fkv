# prawf

## lleol

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Defnyddio cofrestrfa fyd-eang a rennir achosion; ychwanegu wrth ddefnyddio `cargo test` yn uniongyrchol
`--test-threads=1`.

## Cynwysyddion Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Mae'r sgript yn defnyddio zigbuild i groes-grynhoi'r prawf deuaidd Linux, ac yna'n ei roi yn y ddelwedd Debian leiaf i'w rhedeg.

## CI

`ci.yml` Rhedeg profion fmt, clippy, llawn a phrofion integreiddio RustFS S3.
`linux-test.yml` Dychwelyd bob nos yn Ubuntu 24.04 LTS a haerodd 4 tudalen KiB.