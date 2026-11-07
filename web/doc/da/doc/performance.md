# Ydeevne

`fkv_bench` Test skrive, læse og scanne belastninger på et fast nøglerum.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatorer inkluderer gennemløb, latens på hvert trin og peak RSS. frigiv workflow release x86_64 /
`fkv_bench` binær for aarch64.