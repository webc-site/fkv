# mtihani

## mtaa

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Tumia kesi ya sajili ya kimataifa iliyoshirikiwa; ongeza unapotumia `cargo test` moja kwa moja
`--test-threads=1`.

## Vyombo vya Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Hati hutumia zigbuild kukusanya toleo la binary la jaribio la Linux, na kisha kuiweka kwenye taswira ndogo ya Debian ili kuendesha.

## CI

`ci.yml` Endesha fmt, clippy, majaribio kamili na majaribio ya ujumuishaji ya RustFS S3.
`linux-test.yml` Ilirejeshwa kila usiku katika Ubuntu 24.04 LTS na kusisitiza kurasa 4 za KB.