# Rendemento

`fkv_bench` Proba as cargas de escritura, lectura e exploración nun espazo de teclas fixo.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Os indicadores inclúen o rendemento, a latencia en cada etapa e o pico de RSS. lanzamento do fluxo de traballo x86_64 /
`fkv_bench` binario para aarch64.