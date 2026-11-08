# ٹیسٹ

## مقامی

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` کیس مشترکہ عالمی رجسٹری کا استعمال کریں؛ براہ راست `cargo test` استعمال کرتے وقت شامل کریں۔
`--test-threads=1`۔

## لینکس کنٹینرز

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

اسکرپٹ لینکس ٹیسٹ بائنری کو کراس کمپائل کرنے کے لیے zigbuild کا استعمال کرتی ہے، اور پھر اسے چلانے کے لیے کم سے کم Debian امیج میں ڈال دیتی ہے۔

## CI

`ci.yml` fmt، clippy، مکمل ٹیسٹ اور RustFS S3 انٹیگریشن ٹیسٹ چلائیں۔