# Performance

`fkv_bench` Sulayi ang pagsulat, pagbasa, ug pag-scan sa mga load sa usa ka fixed key space.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Ang mga timailhan naglakip sa throughput, latency sa matag yugto, ug peak RSS. buhian workflow release x86_64 /
`fkv_bench` binary para sa aarch64.