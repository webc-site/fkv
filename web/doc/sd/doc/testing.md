# ٽيسٽ

## مقامي

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` استعمال ڪريو ڪيس شيئر ٿيل گلوبل رجسٽري؛ شامل ڪريو جڏهن استعمال ڪريو `cargo test` سڌو
`--test-threads=1`.

## لينڪس ڪنٽينرز

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

اسڪرپٽ zigbuild استعمال ڪري ٿو لينڪس ٽيسٽ بائنري کي پار ڪرڻ لاءِ، ۽ پوءِ ان کي هلائڻ لاءِ گھٽ ۾ گھٽ ديبين تصوير ۾ وجهي ٿو.

## CI

`ci.yml` هلائي fmt، ڪلپي، مڪمل ٽيسٽ ۽ RustFS S3 انٽيگريشن ٽيسٽ.