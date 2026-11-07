# ಪರೀಕ್ಷೆ

## ಸ್ಥಳೀಯ

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` ಕೇಸ್ ಹಂಚಿಕೆಯ ಜಾಗತಿಕ ನೋಂದಾವಣೆ ಬಳಸಿ; ನೇರವಾಗಿ `cargo test` ಬಳಸುವಾಗ ಸೇರಿಸಿ
`--test-threads=1`.

## ಲಿನಕ್ಸ್ ಕಂಟೈನರ್‌ಗಳು

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

ಲಿನಕ್ಸ್ ಪರೀಕ್ಷಾ ಬೈನರಿಯನ್ನು ಕ್ರಾಸ್-ಕಂಪೈಲ್ ಮಾಡಲು ಸ್ಕ್ರಿಪ್ಟ್ ಜಿಗ್‌ಬಿಲ್ಡ್ ಅನ್ನು ಬಳಸುತ್ತದೆ ಮತ್ತು ನಂತರ ಅದನ್ನು ಚಲಾಯಿಸಲು ಕನಿಷ್ಠ ಡೆಬಿಯನ್ ಇಮೇಜ್‌ಗೆ ಇರಿಸುತ್ತದೆ.

## CI

`ci.yml` fmt, clippy, ಪೂರ್ಣ ಪರೀಕ್ಷೆಗಳು ಮತ್ತು RustFS S3 ಏಕೀಕರಣ ಪರೀಕ್ಷೆಗಳನ್ನು ರನ್ ಮಾಡಿ.