# សាកល្បង

## ក្នុងស្រុក

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` ប្រើករណីដែលបានចែករំលែកការចុះបញ្ជីសកល; បន្ថែមនៅពេលប្រើ `cargo test`ដោយផ្ទាល់
`--test-threads=1`។

## ធុងផ្ទុកលីនុច

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

ស្គ្រីបប្រើ zigbuild ដើម្បីឆ្លងកាត់ការចងក្រងប្រព័ន្ធគោលពីរសាកល្បងលីនុច ហើយបន្ទាប់មកដាក់វាចូលទៅក្នុងរូបភាពដេបៀនតិចតួចបំផុតដើម្បីដំណើរការ។

## CI

`ci.yml`ដំណើរការ fmt, clippy, ការធ្វើតេស្តពេញលេញ និងការធ្វើតេស្តរួមបញ្ចូល RustFS S3 ។
`linux-test.yml` បានត្រឡប់មកវិញនៅពេលយប់នៅក្នុង Ubuntu 24.04 LTS និងបានអះអាងទំព័រ 4 KiB ។