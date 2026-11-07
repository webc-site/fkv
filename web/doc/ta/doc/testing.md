# சோதனை

## உள்ளூர்

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` கேஸ் பகிரப்பட்ட உலகளாவிய பதிவேட்டைப் பயன்படுத்தவும்; நேரடியாக `cargo test`ஐப் பயன்படுத்தும் போது சேர்க்கவும்
`--test-threads=1`.

## லினக்ஸ் கொள்கலன்கள்

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

லினக்ஸ் சோதனை பைனரியை குறுக்கு-தொகுக்க ஸ்கிரிப்ட் ஜிக்பில்டைப் பயன்படுத்துகிறது, பின்னர் அதை இயக்க குறைந்தபட்ச டெபியன் படத்தில் வைக்கிறது.

## CI

`ci.yml` fmt, clippy, முழு சோதனைகள் மற்றும் RustFS S3 ஒருங்கிணைப்பு சோதனைகளை இயக்கவும்.