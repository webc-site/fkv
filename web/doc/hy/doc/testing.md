# փորձարկում

## տեղական

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Օգտագործեք գործի ընդհանուր գլոբալ ռեգիստրը; ավելացնել `cargo test` ուղղակիորեն օգտագործելիս
`--test-threads=1`:

## Linux կոնտեյներներ

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Սցենարն օգտագործում է zigbuild՝ Linux թեստային երկուականը խաչաձև կազմելու համար, այնուհետև այն տեղադրում է նվազագույն Debian պատկերի մեջ՝ գործարկելու համար:

## CI

`ci.yml` Գործարկեք fmt, clippy, ամբողջական թեստեր և RustFS S3 ինտեգրման թեստեր: