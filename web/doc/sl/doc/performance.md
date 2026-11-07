# Zmogljivost

`fkv_bench` Preskusno pisanje, branje in pregledovanje obremenitev na fiksnem ključnem prostoru.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Kazalniki vključujejo prepustnost, zakasnitev na vsaki stopnji in najvišjo vrednost RSS. izdaja izdaje poteka dela x86_64 /
`fkv_bench` binarno za aarch64.