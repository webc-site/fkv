# পরীক্ষা

## স্থানীয়

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` কেস শেয়ার করা গ্লোবাল রেজিস্ট্রি ব্যবহার করুন; সরাসরি `cargo test` ব্যবহার করার সময় যোগ করুন
`--test-threads=1`।

## লিনাক্স পাত্রে

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

লিনাক্স পরীক্ষা বাইনারি ক্রস-কম্পাইল করার জন্য স্ক্রিপ্টটি জিগবিল্ড ব্যবহার করে এবং তারপর এটি চালানোর জন্য ন্যূনতম ডেবিয়ান ইমেজে রাখে।

## CI

`ci.yml` এফএমটি, ক্লিপি, সম্পূর্ণ পরীক্ষা এবং রাস্টএফএস এস3 ইন্টিগ্রেশন পরীক্ষা চালান।