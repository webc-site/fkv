# Desempenho

`fkv_bench` Teste cargas de gravação, leitura e varredura em um espaço de chave fixa.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Os indicadores incluem rendimento, latência em cada estágio e pico de RSS. liberar liberação de fluxo de trabalho x86_64 /
`fkv_bench` binário para aarch64.