# ביצועים

`fkv_bench` בדוק עומסי כתיבה, קריאה וסריקה על שטח מפתח קבוע.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

האינדיקטורים כוללים תפוקה, חביון בכל שלב ושיא RSS. שחרור זרימת עבודה מהדורת x86_64 /
`fkv_bench` בינארי עבור aarch64.