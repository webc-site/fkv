# Pagganap

`fkv_bench` Subukang magsulat, magbasa, at mag-scan ng mga load sa isang nakapirming key space.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Kasama sa mga tagapagpahiwatig ang throughput, latency sa bawat yugto, at peak RSS. release workflow release x86_64 /
`fkv_bench` binary para sa aarch64.