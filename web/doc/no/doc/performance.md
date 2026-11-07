# Ytelse

`fkv_bench` Testskrive, lese og skanne laster på en fast nøkkelplass.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatorer inkluderer gjennomstrømning, latens på hvert trinn og topp RSS. frigjør arbeidsflytutgivelse x86_64 /
`fkv_bench` binær for aarch64.