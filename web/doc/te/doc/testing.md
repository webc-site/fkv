# పరీక్ష

## స్థానిక

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` భాగస్వామ్య గ్లోబల్ రిజిస్ట్రీని ఉపయోగించండి; నేరుగా `cargo test`ని ఉపయోగిస్తున్నప్పుడు జోడించండి
`--test-threads=1`.

## Linux కంటైనర్లు

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Linux టెస్ట్ బైనరీని క్రాస్-కంపైల్ చేయడానికి స్క్రిప్ట్ జిగ్‌బిల్డ్‌ని ఉపయోగిస్తుంది మరియు దానిని అమలు చేయడానికి కనీస డెబియన్ ఇమేజ్‌లో ఉంచుతుంది.

## CI

`ci.yml` fmt, క్లిప్పీ, పూర్తి పరీక్షలు మరియు RustFS S3 ఇంటిగ్రేషన్ పరీక్షలను అమలు చేయండి.