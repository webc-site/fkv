# परीक्षण

## स्थानीय

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` केस साझा विश्वव्यापी रजिस्ट्री प्रयोग गर्नुहोस्; `cargo test` सिधै प्रयोग गर्दा थप्नुहोस्
`--test-threads=1`।

## लिनक्स कन्टेनरहरू

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

स्क्रिप्टले लिनक्स परीक्षण बाइनरी क्रस-कम्पाइल गर्न zigbuild प्रयोग गर्दछ, र त्यसपछि यसलाई चलाउनको लागि न्यूनतम डेबियन छविमा राख्छ।

## CI

`ci.yml` fmt, clippy, पूर्ण परीक्षण र RustFS S3 एकीकरण परीक्षणहरू चलाउनुहोस्।