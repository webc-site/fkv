# Performance

`fkv_bench` Test write, read, and scan loads on a fixed key space.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indicators include throughput, latency at each stage, and peak RSS. release workflow release x86_64 /
`fkv_bench` binary for aarch64.