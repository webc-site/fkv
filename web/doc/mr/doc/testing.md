# चाचणी

## स्थानिक

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` केस शेअर केलेली ग्लोबल रेजिस्ट्री वापरा; थेट `cargo test` वापरताना जोडा
`--test-threads=1`.

## लिनक्स कंटेनर

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

लिनक्स चाचणी बायनरी क्रॉस-कंपाइल करण्यासाठी स्क्रिप्ट zigbuild वापरते, आणि नंतर चालवण्यासाठी किमान डेबियन इमेजमध्ये ठेवते.

## CI

`ci.yml` fmt, क्लिपी, पूर्ण चाचण्या आणि RustFS S3 एकत्रीकरण चाचण्या चालवा.