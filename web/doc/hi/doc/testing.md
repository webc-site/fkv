# परीक्षा

## स्थानीय

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` केस शेयरिंग वैश्विक रजिस्ट्री का उपयोग करें; `cargo test` का सीधे उपयोग करते समय जोड़ें
<बी आईडी='1'>.

## लिनक्स कंटेनर

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

स्क्रिप्ट लिनक्स परीक्षण बाइनरी को क्रॉस-कंपाइल करने के लिए ज़िगबिल्ड का उपयोग करती है, और फिर इसे चलाने के लिए न्यूनतम डेबियन छवि में डालती है।

## CI

`ci.yml` fmt, clippy, पूर्ण परीक्षण और RustFS S3 एकीकरण परीक्षण चलाएँ।