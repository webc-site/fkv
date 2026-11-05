# Leeschtung

`fkv_bench` Test schreiwen, liesen a scannt Lasten op engem fixen Schlësselraum.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Indikatoren enthalen Duerchgang, Latenz op all Etapp, a Peak RSS. Fräisetzung Workflow Verëffentlechung x86_64 /
`fkv_bench` binär fir aarch64.