# Utendaji

`fkv_bench` Jaribu kuandika, kusoma na kuchanganua mizigo kwenye nafasi ya ufunguo usiobadilika.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Viashirio ni pamoja na matokeo, muda wa kusubiri katika kila hatua, na kilele cha RSS. toa toleo la mtiririko wa kazi x86_64 /
`fkv_bench` binary kwa aarch64.