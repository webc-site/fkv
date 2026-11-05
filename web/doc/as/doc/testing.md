# পৰীক্ষা

## স্থানীয়

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` কেছ অংশীদাৰী গোলকীয় পঞ্জীয়ন ব্যৱহাৰ কৰক; `cargo test` প্ৰত্যক্ষভাৱে ব্যৱহাৰ কৰাৰ সময়ত যোগ কৰক
`--test-threads=1`.

## লিনাক্স ধাৰকসমূহ

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

স্ক্ৰিপ্টে Linux পৰীক্ষা বাইনাৰী ক্ৰছ-কম্পাইল কৰিবলে zigbuild ব্যৱহাৰ কৰে, আৰু তাৰ পিছত ইয়াক চলাবলে নূন্যতম ডেবিয়ান প্ৰতিমুৰ্তিত ৰাখে ।

## CI

`ci.yml` fmt, clippy, সম্পূৰ্ণ পৰীক্ষাসমূহ আৰু RustFS S3 সংহতি পৰীক্ষাসমূহ চলাওক।
`linux-test.yml` উবুন্টু ২৪.০৪ এলটিএছত নিশাৰ ভাগত ঘূৰি আহিছিল আৰু ৪ কিবি পৃষ্ঠা দৃঢ় কৰিছিল।