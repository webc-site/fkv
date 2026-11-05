# katsetada

## kohalik

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Kasutage juhtumiga jagatud globaalset registrit; lisada, kui kasutate otse `cargo test`
`--test-threads=1`.

## Linuxi konteinerid

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Skript kasutab Linuxi testbinaarfaili ristkompileerimiseks zigbuildi ja lisab selle seejärel käitamiseks minimaalsesse Debiani kujutisse.

## CI

`ci.yml` Käivitage fmt, clippy, täielikud testid ja RustFS S3 integratsioonitestid.
`linux-test.yml` Tagastati igal õhtul Ubuntu 24.04 LTS-is ja kinnitati 4 KiB lehekülge.