# පරීක්ෂණය

## දේශීය

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` භාවිත කේස් බෙදාගත් ගෝලීය ලේඛනය; සෘජුවම `cargo test` භාවිතා කරන විට එකතු කරන්න
`--test-threads=1`.

## ලිනක්ස් බහාලුම්

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

ලිනක්ස් පරීක්ෂණ ද්විමය හරස් සම්පාදනය කිරීමට ස්ක්‍රිප්ට් zigbuild භාවිතා කරයි, පසුව එය ක්‍රියාත්මක කිරීමට අවම ඩේබියන් රූපයට තබයි.

## CI

`ci.yml` fmt, clippy, සම්පූර්ණ පරීක්ෂණ සහ RustFS S3 ඒකාබද්ධතා පරීක්ෂණ ධාවනය කරන්න.