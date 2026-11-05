# pagsubok

## lokal

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Use case shared global registry; idagdag kapag direktang ginagamit ang `cargo test`
`--test-threads=1`.

## Mga lalagyan ng Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Gumagamit ang script ng zigbuild upang i-cross-compile ang Linux test binary, at pagkatapos ay inilalagay ito sa minimal na imahe ng Debian upang tumakbo.

## CI

`ci.yml` Magpatakbo ng fmt, clippy, buong mga pagsubok at mga pagsubok sa pagsasama ng RustFS S3.
`linux-test.yml` Ibinabalik gabi-gabi sa Ubuntu 24.04 LTS at iginiit ang 4 na KiB na pahina.