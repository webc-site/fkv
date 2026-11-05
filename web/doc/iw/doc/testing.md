# מִבְחָן

## מְקוֹמִי

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` שימוש ברישום גלובלי משותף; הוסף בעת שימוש ישירות ב`cargo test`
`--test-threads=1`.

## מיכלי לינוקס

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

הסקריפט משתמש ב-zigbuild כדי להרכיב את הבינארי של מבחן לינוקס, ולאחר מכן מכניס אותו לתמונה המינימלית של דביאן להפעלה.

## CI

`ci.yml` הפעל בדיקות fmt, clippy, מלאות ומבחני שילוב RustFS S3.
`linux-test.yml` חזר מדי לילה באובונטו 24.04 LTS וטען 4 דפי KiB.